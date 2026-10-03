# Spike: exec-form hooks without a shell (#21)

Not product code; removed when #21 is decided. `.github/workflows/spike-exec-hooks.yml` lays out
a plugin `libexec/` (the real `sh` launcher, a stub binary per target, and on Windows a stub
`weighin.exe` beside the launcher) and starts it the way Claude Code 2.1.288 starts an exec-form
hook: `child_process.spawn` under the latest public Bun (Claude Code embeds 1.4.3, which is not published), with the hook
JSON on stdin. Each case prints a `RESULT` line.
