# Spike: exec-form hooks without a shell (#21)

Not product code; removed when #21 is decided. `.github/workflows/spike-exec-hooks.yml` lays out
a plugin `libexec/` (the real `sh` launcher, a stub binary per target, and on Windows a stub
`weighin.exe` beside the launcher) and starts it the way Claude Code 2.1.288 starts an exec-form
hook: `child_process.spawn` under Bun 1.4.3 (the Bun embedded in that Claude Code), with the hook
JSON on stdin. Each case prints a `RESULT` line.
