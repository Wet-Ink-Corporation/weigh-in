# Eval seed for marker capture (#22)

The design's slice 1 exit criterion is marker capture precision of at least 80% on a labelled
seed ([v1 plan](../docs/v1-plan.md), Phase 3). `crates/weighin/tests/eval_seed.rs` scores a seed
by running each item through the real capture path in a scratch repository: a `Stop` hook for
`channel: marker`, a `PostToolUse` Write of a markdown file for `channel: scan`.

- **precision** = captured items labelled `capture` / captured items
- **recall** = captured items labelled `capture` / items labelled `capture`
- **status** = captured `capture` items whose first ledger item has `expect.status`

```sh
cargo test -p weighin --test eval_seed -- --nocapture
```

## Format

One JSON object per line:

```json
{"id":"s2-01","session":"s2","source":"<session id>","repo":"Wet-Ink-Corporation/weigh-in",
 "origin":"real-message","channel":"marker","text":"<the assistant message, verbatim, trimmed>",
 "label":"capture","expect":{"status":"raised"},"why":"<one line>","by":"claude-draft","on":"2026-10-03"}
```

- `label`: `capture` (a decision the ledger should hold, open or as a logged default) or `skip`
  (noise, or no decision).
- `expect.status`, for `capture` items: `raised` (stays open) or `defaulted` (a judgment-bound
  two-way call the agent acted on).
- `origin`: `real-message` (an assistant message as written), `scripted-block` (a block written onto
  a real message), or `scripted-marker` (a test marker from an install check).
- Text is redacted: no client names, emails, tokens or personal data.

## Files

- `seed-scripted.jsonl`: provisional. None of the five earlier sessions it draws on carried a real
  block (capture was off in one, #38), so its precision items are blocks written onto real
  messages, and its precision follows from how many noise blocks were written. The test pins its
  counts, so a change to what capture raises or defaults shows up.
- `seed.jsonl` (to come): the real seed, drafted from blocks agents emit during Phase 3's daily
  use (#23) and approved by Ryan. The 80% bar applies to it.
