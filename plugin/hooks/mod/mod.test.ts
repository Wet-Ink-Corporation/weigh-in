// Weigh-In mod (#114), under `claude plugin test plugin`. The test's own hooks answer
// `$.process.run` (the weighin launcher), `$.session.*` and `$.env.get`, so no process runs: each
// case hands the mod what `open --repo --json` and `digest --json` print and reads what it draws.
import { expect, mock, test } from 'claude-code/testing'
import type { On, RenderSurface } from 'claude-code'

const SURFACES = ['terminal', 'desktop'] as const

const BAND = {
  plugin: 'weigh',
  component: 'AbovePrompt',
  props: { hasSurvey: false, isWorking: false, maxRows: 10, bodyColumns: 120, scroll: { offset: 0, bodyRows: 10 }, view: {} },
} as const

const item = (id: string, door: string | null, hint = 'unsure') => ({ id, door, door_hint: hint, status: 'raised' })
const dflt = (id: string, acknowledged = false) => ({ id, status: 'defaulted', default: { option: 'A', acknowledged } })

const OPEN3 = JSON.stringify([item('wi-000001', 'one-way'), item('wi-000002', null, 'one-way'), item('wi-000003', 'two-way', 'one-way')])
const DIGEST = (defaults: unknown[] = []) => JSON.stringify({ defaults, untriaged_from_other_sessions: [] })

type Out = { exitCode?: number; stdout?: string; stderr?: string }
type Answer = (argv: string[]) => Out | Error

/** The world beneath the mod: the CLI's answers, the session, the environment; records each run. */
function world(on: On, opts: { answer?: Answer; open?: string; digest?: string; surfaces?: RenderSurface[]; env?: Record<string, string> } = {}) {
  const runs: string[][] = []
  const status: (string | undefined)[] = []
  const submits: unknown[] = []
  const commands: string[] = []
  const answer: Answer = opts.answer ?? (argv => {
    if (argv.includes('open') && argv.includes('--json')) return { stdout: opts.open ?? OPEN3 }
    if (argv.includes('digest')) return { stdout: opts.digest ?? DIGEST([dflt('wi-000009'), dflt('wi-00000a'), dflt('wi-00000b', true)]) }
    return { stdout: '' }
  })
  on('process.run', async (_$, e) => {
    const argv = [...e.argv]
    runs.push(argv)
    const out = answer(argv)
    if (out instanceof Error) return { deny: out.message } as any
    return { value: { exitCode: out.exitCode ?? 0, stdout: out.stdout ?? '', stderr: out.stderr ?? '', isStdoutTruncated: false, isStderrTruncated: false } }
  })
  on('session.surfaces', async () => ({ value: opts.surfaces ?? ['terminal'] }) as any)
  on('session.id', async () => ({ value: 'test-session' }) as any)
  on('session.root', async () => ({ value: '/w' }) as any)
  on('ui.status', async (_$, e: any) => { status.push(e.text); return { value: undefined } as any })
  on('command.run', { command: 'weigh-in' }, async (_$, e: any) => { submits.push(e); return { text: '' } })
  on('command.register', async (_$, e: any) => { commands.push(e.name); return { value: undefined } as any })
  on('session.start', async (_$, e) => ({ cwd: e.cwd }))
  on('turn.complete', async () => ({ text: '' }))
  on('session.attach', async (_$, e) => ({ clientId: e.clientId }) as any)
  // The engine draws nothing in the band: an empty Box stands for that here.
  on('ui.render', { component: 'AbovePrompt' }, async () => h('Box', {}) as any)
  mock.env(on, opts.env ?? {})
  return { runs, status, submits, commands }
}

const start = ($: any, surface: RenderSurface | null = 'terminal') => $.session.start({ cwd: '/w', surface, isInteractive: true })

/** A main-loop turn ends; the mod refreshes once its hook has returned, so the clock moves on by 0 ms. */
async function turnEnd($: any, clock: { advance: (ms: number) => Promise<void> }, turnId = 't1', agentId?: string) {
  await $.turn.complete({ answer: '', durationMs: 1, isAborted: false, turnId, reason: 'answer', ...(agentId ? { agentId } : {}) })
  await clock.advance(0)
}

async function band($: any, surface: RenderSurface, props: Record<string, unknown> = {}) {
  const ui = await $.ui.mount({ ...BAND, surface, props: { ...BAND.props, ...props } })
  const texts = (await ui.findAll({ type: 'Text' })).map((t: { text: string }) => t.text).join('')
  const sit = await ui.find({ key: 'sit' })
  return { ui, texts, sit }
}

async function bandText($: any, surface: RenderSurface, props: Record<string, unknown> = {}): Promise<string> {
  const { ui, texts } = await band($, surface, props)
  await ui.unmount()
  return texts
}

const wi = ($: any, args: string) => $.command.run({ command: 'wi', args })

for (const surface of SURFACES) {
  test(`band on ${surface}: N open, M one-way, K defaults, and Sit`, async ($, on) => {
    const w = world(on, { surfaces: [surface] })
    await start($, surface)
    const { ui, texts, sit } = await band($, surface)
    expect(texts).toBe('Weigh-In · 3 open here · 2 one-way · 2 defaults ')
    expect(sit?.text).toContain('Sit')
    expect(await ui.find({ key: 'board' })).toBeUndefined()
    await ui.press({ key: 'sit' })
    expect(w.submits).toHaveLength(1)
    expect(w.submits[0]).toMatchObject({ command: 'weigh-in', args: '' })
    await ui.unmount()
    // One refresh: both documents, with the skill's `W` globals, through this plugin's launcher.
    expect(w.runs).toHaveLength(2)
    for (const run of w.runs) {
      expect(run[0]).toMatch(/\/libexec\/weighin$/)
      expect(run.slice(1, 5)).toEqual(['--session', 'test-session', '--dir', '/w'])
    }
    expect(w.runs.map(r => r.slice(5, 8))).toEqual([['open', '--repo', '--json'], ['digest', '--json']])
    expect(w.runs[0]?.slice(8, 9)).toEqual(['--top'])
    expect(w.commands).toEqual(['wi'])
    expect(w.status).toEqual([])
  })

  test(`band on ${surface}: hidden at 0`, async ($, on) => {
    const w = world(on, { surfaces: [surface], open: '[]', digest: DIGEST([dflt('wi-00000b', true)]) })
    await start($, surface)
    const { ui, texts, sit } = await band($, surface)
    expect(texts).toBe('')
    expect(sit).toBeUndefined()
    await ui.unmount()
    expect(w.status).toEqual([])
  })

  test(`band on ${surface}: defaults alone still show`, async ($, on) => {
    world(on, { surfaces: [surface], open: '[]', digest: DIGEST([dflt('wi-000009')]) })
    await start($, surface)
    expect(await bandText($, surface)).toBe('Weigh-In · 0 open here · 0 one-way · 1 defaults ')
  })

  test(`band on ${surface}: narrow, the short line; too narrow, the status entry instead`, async ($, on) => {
    const w = world(on, { surfaces: [surface] })
    await start($, surface)
    expect(await bandText($, surface, { bodyColumns: 50 })).toBe('Weigh-In · 3 open · 2 1-way · 2 dflt ')
    expect(w.status).toEqual([])
    expect(await bandText($, surface, { bodyColumns: 30 })).toBe('')
    expect(w.status).toEqual(['Weigh-In · 3 open here · 2 one-way · 2 defaults'])
    expect(await bandText($, surface)).toBe('Weigh-In · 3 open here · 2 one-way · 2 defaults ')
    expect(w.status).toEqual(['Weigh-In · 3 open here · 2 one-way · 2 defaults', undefined])
  })

  test(`band on ${surface}: a survey holds the band, so the counts go to the status entry`, async ($, on) => {
    const w = world(on, { surfaces: [surface] })
    await start($, surface)
    expect(await bandText($, surface, { hasSurvey: true })).toBe('')
    expect(w.status).toEqual(['Weigh-In · 3 open here · 2 one-way · 2 defaults'])
  })

  test(`band on ${surface}: a failing weighin draws one line with its error`, async ($, on) => {
    world(on, { surfaces: [surface], answer: () => ({ exitCode: 1, stderr: "weighin: loading this folder's config: bad json\n" }) })
    await start($, surface)
    expect(await bandText($, surface)).toBe("Weigh-In: ledger unavailable: loading this folder's config: bad json")
  })

  test(`band on ${surface}: a spawn that cannot start draws one line`, async ($, on) => {
    world(on, { surfaces: [surface], answer: () => new Error('spawn EACCES') })
    await start($, surface)
    expect(await bandText($, surface)).toContain('Weigh-In: weighin did not start')
  })

  test(`band on ${surface}: output that does not parse draws one line`, async ($, on) => {
    world(on, { surfaces: [surface], open: '{ not json' })
    await start($, surface)
    expect(await bandText($, surface)).toBe('Weigh-In: ledger unavailable: weighin printed what does not parse')
  })

  test(`band on ${surface}: no binary says capture is off, as B-11 does`, async ($, on) => {
    world(on, {
      surfaces: [surface],
      answer: () => ({ exitCode: 127, stderr: 'weighin: this copy has no binaries: it is the development tree. (/p/libexec)\n' }),
    })
    await start($, surface)
    expect(await bandText($, surface)).toBe('Weigh-In: capture is off: this copy has no binaries: it is the development tree. (/p/libexec)')
  })
}

test('a turn ending refreshes once; the same turn again, and a subagent turn, do not', async ($, on) => {
  const clock = mock.clock(on)
  let open = OPEN3
  const w = world(on, { answer: argv => (argv.includes('digest') ? { stdout: DIGEST() } : { stdout: open }) })
  await start($)
  expect(w.runs).toHaveLength(2)
  expect(await bandText($, 'terminal')).toContain('3 open here')
  open = JSON.stringify([item('wi-000001', 'one-way')])
  await turnEnd($, clock, 't1')
  expect(w.runs).toHaveLength(4)
  expect(await bandText($, 'terminal')).toContain('1 open here · 1 one-way · 0 defaults')
  await turnEnd($, clock, 't1')
  await turnEnd($, clock, 's1', 'agent-1')
  expect(w.runs).toHaveLength(4)
  await turnEnd($, clock, 't2')
  expect(w.runs).toHaveLength(6)
})

test('WEIGHIN_MOD=0: nothing registered, run or drawn', async ($, on) => {
  const clock = mock.clock(on)
  const w = world(on, { env: { WEIGHIN_MOD: '0' } })
  await start($)
  await turnEnd($, clock)
  expect(w.runs).toHaveLength(0)
  expect(w.commands).toEqual([])
  expect(await bandText($, 'terminal')).toBe('')
})

for (const surfaces of [[], ['vscode'], ['mobile']] as RenderSurface[][]) {
  test(`where only [${surfaces.join(', ')}] draws: nothing registered or run`, async ($, on) => {
    const clock = mock.clock(on)
    const w = world(on, { surfaces })
    await start($, surfaces[0] ?? null)
    await turnEnd($, clock)
    expect(w.runs).toHaveLength(0)
    expect(w.commands).toEqual([])
  })
}

test('a desktop client attaching after the start registers /wi and draws the band', async ($, on) => {
  const clock = mock.clock(on)
  let surfaces: RenderSurface[] = []
  const runs: string[][] = []
  const commands: string[] = []
  on('process.run', async (_$, e) => {
    runs.push([...e.argv])
    const stdout = e.argv.includes('digest') ? DIGEST() : OPEN3
    return { value: { exitCode: 0, stdout, stderr: '', isStdoutTruncated: false, isStderrTruncated: false } }
  })
  on('session.surfaces', async () => ({ value: surfaces }) as any)
  on('session.id', async () => ({ value: 'test-session' }) as any)
  on('session.root', async () => ({ value: '/w' }) as any)
  on('command.register', async (_$, e: any) => { commands.push(e.name); return { value: undefined } as any })
  on('session.start', async (_$, e) => ({ cwd: e.cwd }))
  on('session.attach', async (_$, e) => ({ clientId: e.clientId }) as any)
  on('ui.render', { component: 'AbovePrompt' }, async () => h('Box', {}) as any)
  mock.env(on, {})
  await start($, null)
  expect(runs).toHaveLength(0)
  surfaces = ['desktop']
  await $.session.attach({ surface: 'desktop', clientId: 'desktop:default' })
  await clock.advance(0)
  expect(commands).toEqual(['wi'])
  expect(runs).toHaveLength(2)
  expect(await bandText($, 'desktop')).toContain('3 open here')
})

test('on Windows the launcher is libexec/weighin.exe, else libexec/weighin.cmd', async ($, on) => {
  let hasExe = true
  const w = world(on, { env: { OS: 'Windows_NT' } })
  on('fs.stat', async (_$, e: any) => {
    if (hasExe && String(e.path).endsWith('/libexec/weighin.exe')) return { value: { kind: 'file', size: 1, mtimeMs: 0, isLink: false } } as any
    throw new Error('ENOENT')
  })
  await start($)
  expect(w.runs[0]?.[0]).toMatch(/\/libexec\/weighin\.exe$/)
  hasExe = false
  await wi($, 'stats')
  expect(w.runs.at(-1)?.[0]).toMatch(/\/libexec\/weighin\.cmd$/)
})

// `/wi`: each command runs the CLI by argv, with no shell and no Claude turn, and answers with
// the CLI's own text; a failure answers with its error text and exit code.
const COMMANDS: [string, string, string[]][] = [
  ['open', 'open', ['open', '--repo']],
  ['stats', 'stats', ['stats']],
  ['decide, plain', 'decide wi-562c91 A', ['decide', 'wi-562c91', 'A']],
  ['decide with a note', 'decide wi-562c91 B keep it short; "quoted" $(not run)', ['decide', 'wi-562c91', 'B', '--note', 'keep it short; "quoted" $(not run)']],
  ['decide as the board copies it', 'decide wi-562c91: A (note: short ttl (for now))', ['decide', 'wi-562c91', 'A', '--note', 'short ttl (for now)']],
  [
    'defer as the board copies it',
    'defer wi-a99e1d — trigger: load test; resolve by: 2026-11-01; default: B',
    ['defer', 'wi-a99e1d', '--flip', 'load test', '--trigger', 'load test', '--by', '2026-11-01', '--default', 'B'],
  ],
  [
    'defer with a note',
    'defer wi-a99e1d — trigger: the vendor answers; resolve by: before-merge:main; default: A (note: ask Sam)',
    ['defer', 'wi-a99e1d', '--flip', 'the vendor answers', '--trigger', 'the vendor answers', '--by', 'before-merge:main', '--default', 'A', '--reason', 'ask Sam'],
  ],
]

for (const [what, args, argv] of COMMANDS) {
  test(`/wi ${what}: runs weighin ${argv[0]} by argv and prints what it printed`, async ($, on) => {
    const w = world(on, { answer: a => (a.includes('--json') ? (a.includes('digest') ? { stdout: DIGEST() } : { stdout: '[]' }) : { stdout: `printed by ${a[5]}\n` }) })
    await start($)
    const before = w.runs.length
    const out = await wi($, args)
    expect(out).toEqual({ text: `printed by ${argv[0]}` })
    const run = w.runs[before]
    expect(run?.[0]).toMatch(/\/libexec\/weighin$/)
    expect(run?.slice(1)).toEqual(['--session', 'test-session', '--dir', '/w', ...argv])
    // A decision or deferral changes the counts: the band refreshes once after it.
    const refreshed = argv[0] === 'decide' || argv[0] === 'defer' ? 2 : 0
    expect(w.runs.length - before).toBe(1 + refreshed)
  })
}

test('/wi decide that fails returns the error text and the exit code', async ($, on) => {
  const w = world(on, {
    answer: a => (a.includes('decide') ? { exitCode: 1, stderr: 'weighin: no single item matches `wi-zzz` (try `weighin open --all`)\n' } : { stdout: a.includes('digest') ? DIGEST() : '[]' }),
  })
  await start($)
  const before = w.runs.length
  expect(await wi($, 'decide wi-zzz A')).toEqual({ text: 'weighin: no single item matches `wi-zzz` (try `weighin open --all`)', exitCode: 1 })
  expect(w.runs.length - before).toBe(1)
})

test('/wi with no binary says capture is off, with the launcher\'s exit code', async ($, on) => {
  world(on, { answer: () => ({ exitCode: 127, stderr: 'weighin: no binary for Linux-riscv64 under /p/libexec; build one\n' }) })
  await start($)
  expect(await wi($, 'stats')).toEqual({ text: 'Weigh-In: capture is off: no binary for Linux-riscv64 under /p/libexec; build one', exitCode: 127 })
})

test('/wi whose spawn cannot start says so', async ($, on) => {
  let fail = false
  world(on, { answer: a => (fail ? new Error('spawn ENOENT') : { stdout: a.includes('digest') ? DIGEST() : '[]' }) })
  await start($)
  fail = true
  const out = await wi($, 'open')
  expect(out.text).toMatch(/^Weigh-In: weighin did not start: .*spawn ENOENT$/)
  expect(out.exitCode).toBe(1)
})

for (const args of ['', 'help', 'open --all', 'decide wi-1', 'defer wi-1 tomorrow', 'board']) {
  test(`/wi ${args || '(nothing)'}: the usage, and nothing run`, async ($, on) => {
    const w = world(on)
    await start($)
    const before = w.runs.length
    const out = await wi($, args)
    expect(out.text).toContain('Usage:')
    expect(out.exitCode).toBe(2)
    expect(w.runs.length).toBe(before)
  })
}
