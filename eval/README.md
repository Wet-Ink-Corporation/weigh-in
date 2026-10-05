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
- `seed.jsonl`: the real seed, which the 80% bar applies to
  (`real_seed_meets_the_precision_bar`). Each item is the last message of a turn, verbatim, from
  a Claude Code cloud session on this repository that ran with the marker convention (0.5.0 and
  later), read back from the session's transcript; with the ledger gone with the container
  (D-0014), the transcript is the only record. `by: claude-draft` until Ryan has reviewed the
  label, then `by: ryan`.

  Drafted 2026-10-04 from three sessions (`s3`, `s5`, `s6`; 51 turns since 0.5.0): 25 items, 7
  labelled `capture`. Capture raises one of them, the only turn that carried a block, so
  precision is 100% on one captured item and recall is 14%. The test passes, but the exit
  criterion counts only once the seed holds at least 10 captured items (wi-a9df17): blocks from
  #23's weeks are added here as they come.

  Added 2026-10-05: three end-of-turn messages (`s7`) from the 0.5.2 release session, all true
  negatives. That session started before the 0.5.2 convention merged, so it counts as before 0.5.2.
  Only its last page of turns could be read: the session was gone from the API before the rest
  were fetched. 28 items, 7 labelled `capture`, 1 captured, so recall is still 14%. No
  session has yet run with the 0.5.2 convention line (one block per call), so recall after 0.5.2
  is not measured.
