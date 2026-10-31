## What changed

<!-- One paragraph: what and why. Link the issue it closes (`Closes #N`). -->

## Gate

- [ ] `cargo fmt --check`, `node .claude/temper/temper.mjs baseline --check`, `cargo test --workspace`,
      `cargo doc --workspace --no-deps` (or `node .claude/temper/temper.mjs gate`, which runs them all)
- [ ] Plugin changed: `claude plugin validate .`, and `SKILL.md` still holds one `V=dev; S=none;`

## Decisions

<!-- Decisions taken or touched (D-/B-/wi- ids), and whether they are recorded in docs/decisions/. "None" is fine. -->

## Release

- [ ] No version change
- [ ] Version bumped in `Cargo.toml`, `plugin/.claude-plugin/plugin.json` and
      `plugin-mod/.claude-plugin/plugin.json`, with its
      `CHANGELOG.md` section
