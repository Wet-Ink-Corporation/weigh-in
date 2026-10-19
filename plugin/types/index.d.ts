// Weigh-In mod: the values its drawing reads from `$.state` (the plugin's contract, #114).
// They are counts read from `weighin open --repo --json` and `weighin digest --json`; the mod
// never opens the ledger itself.

/** The ledger's counts for this repository, as the band shows them. */
export type WeighCounts = {
  /** Items open in this repository (`open --repo`), deferred ones included. */
  open: number
  /** Of those, the ones whose door is one-way (as triaged, else as raised). */
  oneWay: number
  /** Defaults taken without the owner and not yet acknowledged (`digest`). */
  defaults: number
}

/**
 * What the band draws: the counts; `no-binary` when the launcher found no weighin build for
 * this platform (B-11's "capture is off"); `unavailable` when weighin ran and failed, or printed
 * what does not parse. `null` before the first refresh: the band then draws nothing.
 */
export type WeighView =
  | { kind: 'counts'; counts: WeighCounts }
  | { kind: 'no-binary'; line: string }
  | { kind: 'unavailable'; line: string }

declare module 'claude-code' {
  interface PluginState {
    weigh: {
      view: WeighView | null
    }
  }
}
