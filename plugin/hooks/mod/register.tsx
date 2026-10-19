// Weigh-In mod (#114): a band above the prompt with this repository's ledger counts, and `/wi`
// commands answered without a Claude turn.
//
// The ledger has one writer, the weighin CLI: the mod reads `open --repo --json` and
// `digest --json` from this plugin's own launcher and writes only through `decide` and `defer`,
// run by argv with no shell. It observes the events it hooks and returns every `next(e)` result
// unchanged. Where no terminal or desktop draws (the VS Code panel, cloud sessions, Cowork,
// `claude -p`) and under WEIGHIN_MOD=0 it registers nothing and runs nothing.
import { atom, read, update } from 'claude-code'
import type { EngineInterface, ProcessRunResult, Register, RenderSurface } from 'claude-code'

import type { WeighCounts, WeighView } from '../../types'

const view = atom({ plugin: 'weigh', key: 'view' } as const, null)

/** The pane the band's Board button will open once it exists (#115). */
export const BOARD_PANE = 'weigh-in-board'
/** What Sit runs: the skill's own sitting, `/weigh-in`. */
const SIT = 'weigh-in'
/** Surfaces that draw the band. */
const DRAWS: ReadonlySet<RenderSurface> = new Set(['terminal', 'desktop'])
/** As long as a hook gets in hooks.json. */
const REFRESH_TIMEOUT_MS = 10_000
const COMMAND_TIMEOUT_MS = 30_000
/** `open --json` lists `--top` items (12 by default); the band counts every one. */
const ALL = '1000000'
/** The launcher's exit when it finds no binary for this platform. */
const NO_BINARY = 127

const USAGE = [
  'Usage:',
  '  /wi open                     open items in this repository',
  '  /wi stats                    the ledger\'s health numbers',
  '  /wi decide <id> <option> [note]',
  '  /wi decide <id>: <option> (note: …)',
  '  /wi defer <id> — trigger: …; resolve by: …; default: <X> (note: …)',
].join('\n')

// Off for this load: WEIGHIN_MOD=0, or no surface that draws. A reload runs register and
// session.start again, so module variables are enough here.
let off = true
let commandRegistered = false
let refreshing: Promise<void> | null = null
let lastTurn: string | null = null
// The status entry each surface asked for while its band had no room; the one shown.
const wantsStatus = new Map<RenderSurface, string>()
let shownStatus: string | undefined

/** One run of the launcher, as the band and the commands read it. */
export type Run =
  | { kind: 'ran'; result: ProcessRunResult }
  | { kind: 'no-start'; why: string }

/** The launcher for this platform: the Windows build directly, else the sh launcher (B-16). */
async function launcher($: EngineInterface): Promise<string> {
  const libexec = `${$.plugin.root}/libexec`
  if ((await $.env.get('OS')) !== 'Windows_NT') return `${libexec}/weighin`
  try {
    const exe = await $.fs.stat(`${libexec}/weighin.exe`)
    if (exe.kind === 'file') return `${libexec}/weighin.exe`
  } catch {
    // No build at libexec/weighin.exe: the cmd launcher finds one under libexec/<target>/.
  }
  return `${libexec}/weighin.cmd`
}

/** Runs weighin with the skill's `W` globals: this session's key and its project directory. */
async function weighin($: EngineInterface, args: readonly string[], timeoutMs: number): Promise<Run> {
  try {
    const argv = [await launcher($), '--session', await $.session.id(), '--dir', await $.session.root(), ...args]
    return { kind: 'ran', result: await $.process.run(argv, { timeoutMs }) }
  } catch (err) {
    return { kind: 'no-start', why: firstLine(err instanceof Error ? err.message : String(err)) }
  }
}

function firstLine(text: string): string {
  return text.trim().split('\n')[0]?.trim() ?? ''
}

/** B-11's line: the launcher's own reason, without its `weighin:` prefix. */
export function captureOff(stderr: string): string {
  const why = firstLine(stderr).replace(/^weighin:\s*/, '')
  return `Weigh-In: capture is off: ${why || 'no weighin binary for this platform.'}`
}

/** Whether a terminal or the desktop app draws this session. */
async function draws($: EngineInterface): Promise<boolean> {
  return (await $.session.surfaces()).some(s => DRAWS.has(s))
}

/** The counts from the two documents, or why they could not be read. */
export function countsOf(openJson: string, digestJson: string): WeighCounts | null {
  try {
    const items: unknown = JSON.parse(openJson)
    const digest: unknown = JSON.parse(digestJson)
    if (!Array.isArray(items) || typeof digest !== 'object' || digest === null) return null
    const defaults: unknown = (digest as { defaults?: unknown }).defaults
    if (!Array.isArray(defaults)) return null
    const oneWay = items.filter(i => doorOf(i) === 'one-way').length
    const unacked = defaults.filter(d => (d as { default?: { acknowledged?: unknown } } | null)?.default?.acknowledged !== true)
    return { open: items.length, oneWay, defaults: unacked.length }
  } catch {
    return null
  }
}

/** The item's door as triaged, else as raised: `Item::door` in weighin-core. */
function doorOf(item: unknown): unknown {
  const i = item as { door?: unknown; door_hint?: unknown } | null
  return i?.door ?? i?.door_hint
}

/** What a failed run says, for the band: the no-binary line, or the CLI's first error line. */
function failure(run: Run): WeighView {
  if (run.kind === 'no-start') return { kind: 'unavailable', line: `Weigh-In: weighin did not start: ${run.why}` }
  const { exitCode, stderr, stdout } = run.result
  if (exitCode === NO_BINARY) return { kind: 'no-binary', line: captureOff(stderr) }
  const why = firstLine(stderr) || firstLine(stdout) || `exit ${exitCode}`
  return { kind: 'unavailable', line: `Weigh-In: ledger unavailable: ${why.replace(/^weighin:\s*/, '')}` }
}

/** One refresh: both documents, read side by side. */
async function readView($: EngineInterface): Promise<WeighView> {
  const [open, digest] = await Promise.all([
    weighin($, ['open', '--repo', '--json', '--top', ALL], REFRESH_TIMEOUT_MS),
    weighin($, ['digest', '--json'], REFRESH_TIMEOUT_MS),
  ])
  if (open.kind !== 'ran' || open.result.exitCode !== 0) return failure(open)
  if (digest.kind !== 'ran' || digest.result.exitCode !== 0) return failure(digest)
  if (open.result.isStdoutTruncated || digest.result.isStdoutTruncated) {
    return { kind: 'unavailable', line: 'Weigh-In: ledger unavailable: the listing was too long to read' }
  }
  const counts = countsOf(open.result.stdout, digest.result.stdout)
  if (counts === null) return { kind: 'unavailable', line: 'Weigh-In: ledger unavailable: weighin printed what does not parse' }
  return { kind: 'counts', counts }
}

/** Refreshes the band; a refresh already running is the one that answers. */
function refresh($: EngineInterface): Promise<void> {
  if (refreshing) return refreshing
  refreshing = (async () => {
    if (!(await draws($))) return
    const fresh = await readView($)
    await update($, view, () => fresh)
  })().finally(() => {
    refreshing = null
  })
  return refreshing
}

/** `/wi` is listed once a surface that draws is known. */
async function registerCommand($: EngineInterface): Promise<void> {
  if (commandRegistered) return
  commandRegistered = true
  await $.command.register({
    name: 'wi',
    description: 'Weigh-In ledger: open, stats, decide or defer an item, with no Claude turn',
    argumentHint: 'open | stats | decide <id> <option> [note] | defer <id> — trigger: …; resolve by: …; default: <X>',
  })
}

/** What `/wi <args>` runs: the CLI's arguments, or the usage text. */
export function commandArgs(input: string): { args: string[] } | { usage: string } {
  const text = input.trim()
  const [sub = '', ...rest] = text.split(/\s+/)
  const tail = text.slice(sub.length).trim()
  switch (sub) {
    case 'open':
      return rest.length === 0 ? { args: ['open', '--repo'] } : { usage: USAGE }
    case 'stats':
      return rest.length === 0 ? { args: ['stats'] } : { usage: USAGE }
    case 'decide':
      return decideArgs(tail)
    case 'defer':
      return deferArgs(tail)
    default:
      return { usage: USAGE }
  }
}

/** Splits `(note: …)` off the end, as the board writes it. */
function splitNote(text: string): { body: string; note?: string } {
  const m = /^(.*?)\s*\(note:\s*(.*)\)\s*$/s.exec(text)
  if (!m) return { body: text }
  const [, body = '', note = ''] = m
  return note.trim() ? { body, note: note.trim() } : { body }
}

/** `<id> <option> [note]`, or the board's `<id>: <option> (note: …)`. */
function decideArgs(tail: string): { args: string[] } | { usage: string } {
  const { body, note: boardNote } = splitNote(tail)
  const m = /^(\S+?):?\s+(\S+)(?:\s+(.+))?$/s.exec(body.trim())
  if (!m) return { usage: USAGE }
  const [, id = '', option = '', plainNote] = m
  const note = boardNote ?? plainNote?.trim()
  return { args: ['decide', id, option, ...(note ? ['--note', note] : [])] }
}

/** The board's `<id> — trigger: …; resolve by: …; default: <X> (note: …)`, as the skill maps it. */
function deferArgs(tail: string): { args: string[] } | { usage: string } {
  const { body, note } = splitNote(tail)
  const m = /^(\S+?):?\s+(?:—|–|--?)?\s*trigger:\s*(.+?);\s*resolve by:\s*(.+?);\s*default:\s*(.*)$/s.exec(body.trim())
  if (!m) return { usage: USAGE }
  const [, id = '', trigger = '', by = '', fallback = ''] = m
  return {
    args: [
      'defer', id, '--flip', trigger.trim(), '--trigger', trigger.trim(), '--by', by.trim(), '--default', fallback.trim(),
      ...(note ? ['--reason', note] : []),
    ],
  }
}

/** The command's answer: the CLI's own text, and its exit code when it failed. */
export function answer(run: Run): { text: string; exitCode?: number } {
  if (run.kind === 'no-start') return { text: `Weigh-In: weighin did not start: ${run.why}`, exitCode: 1 }
  const { exitCode, stdout, stderr } = run.result
  if (exitCode === NO_BINARY) return { text: captureOff(stderr), exitCode }
  if (exitCode === 0) return { text: stdout.trimEnd() || stderr.trimEnd() }
  const text = [stdout.trimEnd(), stderr.trimEnd()].filter(Boolean).join('\n') || `weighin exited ${exitCode}`
  return { text, exitCode: Math.min(255, Math.max(1, Math.trunc(exitCode))) }
}

/** The band's line, fitted to `columns`, or null when it has no room. */
export function bandLine(c: WeighCounts, columns: number, button: number): string | null {
  const full = `Weigh-In · ${c.open} open here · ${c.oneWay} one-way · ${c.defaults} defaults`
  const short = `Weigh-In · ${c.open} open · ${c.oneWay} 1-way · ${c.defaults} dflt`
  for (const line of [full, short]) {
    if (line.length + button <= columns) return line
  }
  return null
}

/** The status entry: the full line, where the band has no room for it. */
export function statusLine(c: WeighCounts): string {
  return `Weigh-In · ${c.open} open here · ${c.oneWay} one-way · ${c.defaults} defaults`
}

/** Asks for (or withdraws) the status entry for one surface; shows it while any surface asks. */
function wantStatus($: EngineInterface, surface: RenderSurface, text: string | undefined): void {
  if (text === undefined) wantsStatus.delete(surface)
  else wantsStatus.set(surface, text)
  const next = wantsStatus.values().next().value
  if (next === shownStatus) return
  shownStatus = next
  $.ui.status(next)
}

/** `[ Sit ]` and the space before it. */
const SIT_WIDTH = ' [ Sit ]'.length

export const register: Register = on => {
  off = true
  commandRegistered = false
  refreshing = null
  lastTurn = null
  wantsStatus.clear()
  shownStatus = undefined

  on('session.start', async ($, e, next) => {
    const result = await next(e)
    off = (await $.env.get('WEIGHIN_MOD')) === '0'
    if (off || !(await draws($))) return result
    await registerCommand($)
    await refresh($)
    return result
  })

  // A client that draws attached after the start (the desktop app connects as an SDK host, so
  // session.start saw no surface): register and draw now rather than at the first turn end.
  on('session.attach', async ($, e, next) => {
    const result = await next(e)
    if (!off && DRAWS.has(e.surface)) {
      await registerCommand($)
      $.clock.after(0, () => refresh($))
    }
    return result
  })

  // One refresh per main-loop turn, after the hook has returned, so a slow CLI never holds the
  // end of the turn. Subagents' turns end inside it and are not counted.
  on('turn.complete', async ($, e, next) => {
    const result = await next(e)
    if (off || e.agentId !== undefined || e.turnId === lastTurn) return result
    lastTurn = e.turnId
    $.clock.after(0, () => refresh($))
    return result
  })

  on('command.run', { command: 'wi' }, async ($, e) => {
    const parsed = commandArgs(e.args)
    if ('usage' in parsed) return { text: parsed.usage, exitCode: 2 }
    const run = await weighin($, parsed.args, COMMAND_TIMEOUT_MS)
    const out = answer(run)
    const [sub] = parsed.args
    if ((sub === 'decide' || sub === 'defer') && out.exitCode === undefined) await refresh($)
    return out
  })

  on('ui.render', { component: 'AbovePrompt' }, async ($, e, next) => {
    if (off || !DRAWS.has(e.surface)) return next(e)
    const v = await read($, view)
    if (v === null || (v.kind === 'counts' && v.counts.open === 0 && v.counts.defaults === 0)) {
      wantStatus($, e.surface, undefined)
      return next(e)
    }
    if (v.kind !== 'counts') {
      wantStatus($, e.surface, e.props.hasSurvey ? v.line : undefined)
      if (e.props.hasSurvey) return next(e)
      const { Box, Text } = $.ui.resolve(e)
      return (
        <Box>
          <Text dimColor wrap="truncate-end">{v.line}</Text>
        </Box>
      )
    }
    const line = e.props.hasSurvey ? null : bandLine(v.counts, e.props.bodyColumns, SIT_WIDTH)
    if (line === null) {
      wantStatus($, e.surface, statusLine(v.counts))
      return next(e)
    }
    wantStatus($, e.surface, undefined)
    const { Box, Button, Text } = $.ui.resolve(e)
    // The Board button joins Sit here once the pane exists (#115): it opens BOARD_PANE.
    return (
      <Box>
        <Text key="counts" wrap="truncate-end">{line}</Text>
        <Text> </Text>
        <Button key="sit" label="Sit" hotkey="s" variant="primary" onPress={() => sit($)} />
      </Box>
    )
  })
}

/** Starts a sitting: runs the skill's `/weigh-in`, as the person typing it would. */
async function sit($: EngineInterface): Promise<void> {
  await $.command.run({ command: SIT })
}
