## What changed

<!-- One paragraph: what and why. Link the issue it closes (`Closes #N`). -->

## Gate

- [ ] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`,
      `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`
- [ ] Plugin changed: `claude plugin validate .`, and `SKILL.md` still holds one `V=dev; S=none;`

## Decisions

<!-- Decisions taken or touched (D-/B-/wi- ids), and whether they are recorded in docs/decisions/. "None" is fine. -->

## Release

- [ ] No version change
- [ ] Version bumped in both `Cargo.toml` and `plugin/.claude-plugin/plugin.json`, with its
      `CHANGELOG.md` section
