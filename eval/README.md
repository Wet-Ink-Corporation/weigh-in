# Eval seed for marker capture (#22)

The design's slice 1 exit criterion is marker capture precision of at least 80% on a labelled
seed ([v1 plan](../docs/v1-plan.md), Phase 3). `crates/weighin/tests/it/eval_seed.rs` scores a
seed by running each item through the real capture path in a scratch repository: a `Stop` hook for
`channel: marker`, a `PostToolUse` Write of a markdown file for `channel: scan`.

- **precision** = captured items labelled `capture` / captured items
- **recall** = captured items labelled `capture` / items labelled `capture`
- **status** = captured `capture` items whose first ledger item has `expect.status`

```sh
cargo test -p weighin --test it eval_seed -- --nocapture
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

  From 2026-10-05 each session adds its own turns before it ends (wi-48657b, `CLAUDE.md`),
  parsed from its transcript under `~/.claude/projects/`, because a session can leave the
  sessions API before a later one reads it back (the 0.5.2 release session did).

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

  Added 2026-10-05: four turns (`s8`) from the first session on the 0.5.2 convention, saved by
  the session itself. One carries a block and is captured; three are true negatives. The seed is now 32 items, 8
  labelled `capture`, 2 captured (precision 100%, recall 25%). After 0.5.2, recall is 1 of 1 on one
  session, which is too few to judge the new line.

  Added 2026-10-05: one turn (`s9`, `rs9-01`), the hand-off message of the session that followed
  s8. Before that turn the session had no finished turns, so it wrote the message into the seed
  before sending it, word for word, and checks it against its transcript after the sitting. It
  carries a block and is captured as a logged default. No other cloud session on this repository
  since 0.5.0 was missing from the seed, and s3, s5, s7 and s8 had left the sessions API, so none
  could be read again. The seed is now 33 items, 9 labelled `capture`, 3 captured (precision 100%,
  recall 33%). Before 0.5.2 recall is 1 of 7; after it, 2 of 2 (s8, s9). Both are
  messages that report on the seed itself, so they say little about the line on ordinary work.

  After the sitting, the session added its other five end-of-turn messages (`rs9-02` to
  `rs9-06`), parsed from its transcript, and checked `rs9-01` against it (word for word). One
  carries two blocks and is captured as a logged default; four are true negatives. Messages
  written mid-turn, before a tool call or a worker restart, are not end-of-turn messages and are
  left out. The seed is now 38 items, 10 labelled `capture`, 4 captured (precision 100%, recall
  40%). After 0.5.2, recall is 3 of 3 across s8 and s9.

  Later the same day: six more end-of-turn messages from s9 (`rs9-07` to `rs9-12`), all true
  negatives (the sitting's close, CI and merge notices, a check result). The seed is 44 items, 10
  labelled `capture`, 4 captured (precision 100%, recall 40%).

  Added 2026-10-06: six end-of-turn messages from s10, the temper remediation session (PR #82),
  saved by the session itself (`rs10-01` to `rs10-06`). Three carry blocks: calls the agent acted
  on, plus one deferral, so they are captured as logged defaults. Three are true negatives. The
  seed is 50 items, 13 labelled `capture`, 7 captured (precision 100%, recall 54%). After 0.5.2,
  recall is 6 of 6 across s8, s9 and s10.
