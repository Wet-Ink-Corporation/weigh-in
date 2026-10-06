# Installing Weigh-In

For using Weigh-In. Building it from source, for development, is in the
[README](../README.md#install). Releases are internal for v1 (D-0012), so every route below needs
read access to `Wet-Ink-Corporation/weigh-in`.

## Through your organization (recommended)

Each release is pushed to `Wet-Ink-Corporation/weigh-in-plugin`, a release-only repository
holding the built plugin with its Linux, macOS and Windows binaries (B-19). An organization that
syncs it gets every release in Cowork and in Claude Code with no step of anyone's. Once, as an
owner:

1. Create `Wet-Ink-Corporation/weigh-in-plugin`, private, with no files.
2. Make an SSH key pair (`ssh-keygen -t ed25519 -N "" -f weigh-in-plugin`). Add the public key to
   that repository under Settings > Deploy keys, with write access, and the private key to
   `Wet-Ink-Corporation/weigh-in` under Settings > Secrets and variables > Actions as
   `WEIGH_IN_PLUGIN_DEPLOY_KEY`. The release refuses to publish without it.
3. On claude.ai, Organization settings > Plugins & skills: sync from GitHub
   `Wet-Ink-Corporation/weigh-in-plugin` (not `weigh-in`, whose `main` is the unreleased tree),
   leave **Sync automatically** on, and set `weigh` to installed by default or required.
4. Remove other installs, so `/weigh-in` appears once: an uploaded zip under Customize >
   Plugins, and in Claude Code `/plugin marketplace remove weigh-in` if you added the
   `#marketplace` branch (below).

The first release after step 2 fills the repository; until then there is nothing to sync.

## Claude Code without the organization sync

Each release publishes a `marketplace` branch: one commit holding the plugin with its Linux,
macOS (Apple silicon) and Windows binaries (D-0009). No Rust toolchain is needed.

1. Let git reach the private repository without prompting. With the GitHub CLI:

   ```sh
   gh auth login
   gh auth setup-git
   ```

   Background updates run git with no prompt, through a credential helper (`gh`, the macOS
   Keychain, Git Credential Manager) or an SSH key in `ssh-agent`. A `GITHUB_TOKEN` in the
   environment is not enough on its own.
2. In Claude Code:

   ```
   /plugin marketplace add Wet-Ink-Corporation/weigh-in#marketplace
   /plugin install weigh@weigh-in
   ```

3. Turn on auto-update: `/plugin` → **Marketplaces** → `weigh-in` → **Enable auto-update**.
   Claude Code leaves it off for every marketplace that is not an official one or added from
   claude.ai, so without this step releases arrive only when you update by hand.
4. Start a new session. Type `/weigh-in` (Claude Code also lists it as `/weigh:weigh-in`). In a
   client repository, run `/weigh-in init client` first, which keeps the board local.

Hooks need no shell: Claude Code starts `libexec/weighin` directly (exec form), which on Windows
resolves to `libexec/weighin.exe`, so capture works with or without Git for Windows (B-16). The
sitting is different: `/weigh-in` runs `weighin` through `sh`, so on Windows it needs
[Git for Windows](https://git-scm.com/downloads/win) (Git Bash). Without it, Claude Code uses its
PowerShell tool and the sitting's commands fail with `sh` not found; if Git is installed but not
found, set `CLAUDE_CODE_GIT_BASH_PATH` (Claude Code's
[setup guide](https://code.claude.com/docs/en/setup#set-up-on-windows)).

### Update

With auto-update on (step 3), Claude Code checks the marketplace in the background and re-clones
it when it has new commits, so a release reaches you without a step of your own. To update now,
or with auto-update off:

```
/plugin marketplace update weigh-in
```

Keep one install. A local build (`/plugin marketplace add <path to a checkout>`) or a plugin
uploaded to your claude.ai account beside this one runs a second copy, and two versions should
not share a ledger (B-05). If `/weigh-in` shows twice, remove one.

### Uninstall

```
/plugin uninstall weigh@weigh-in
/plugin marketplace remove weigh-in
```

The plugin's files are under `~/.claude/plugins/cache/weigh-in/`. Uninstalling leaves every
ledger and every `.kb/` atom where it is; delete a ledger's folder (below) yourself if you want
it gone.

## Cowork

Cowork takes plugins from your claude.ai account, and while releases are internal its skill
cannot download the binary, so it uses one in a connected folder (D-0012).

1. Keep a checkout of the `marketplace` branch:

   ```sh
   git clone --branch marketplace --single-branch https://github.com/Wet-Ink-Corporation/weigh-in.git weigh-in-marketplace
   ```

   After each release:

   ```sh
   git -C weigh-in-marketplace fetch origin marketplace
   git -C weigh-in-marketplace reset --hard origin/marketplace
   ```

   The branch is replaced each release, so `git pull` would refuse to merge the unrelated
   commit; the reset takes it. Keep nothing of your own in this checkout.
2. Connect that folder to Cowork once. The skill finds
   `plugin/libexec/x86_64-unknown-linux-musl/weighin` in it.
3. With the organization sync (above), the plugin is already there. Without it, upload
   `weigh-in-plugin-<version>.zip` from the
   [release](https://github.com/Wet-Ink-Corporation/weigh-in/releases) under Customize >
   Plugins, and again for each release.

The plugin you upload also appears in Claude Code. If you installed from the marketplace branch
too, you will see `/weigh-in` twice; remove one.

To uninstall, remove the plugin under Customize > Plugins and disconnect the folder.

## Where the ledger is

First match wins:

1. `--store <file>` on the command line.
2. `$WEIGHIN_STORE`.
3. `store` in `.kb/_ledger/config.json`, set with `weighin init --ledger <path>`; a relative path
   is taken from the repository root.
4. In a git repository: `<git common dir>/weighin/ledger.sqlite3`, usually
   `.git/weighin/ledger.sqlite3`. All worktrees share it, and it is never committed.
5. Outside git (a Cowork folder): `<folder>/.weighin/ledger.sqlite3`.

A ledger is created by the first item raised; a session that raises nothing leaves none behind.
Decisions are also written as atoms under `.kb/`, which you commit; the ledger holds the
history and the open items. If a synced folder (OneDrive, Dropbox) is used from more than one
computer, point `store` outside the synced tree.

## `hook-errors.log`

Hooks never fail a session. When one fails, it appends a line to `hook-errors.log` in the
ledger's folder (step 4 above: `.git/weighin/hook-errors.log`) and exits quietly:

```
<time, RFC 3339 UTC> <hook>: <error>: <cause>: …
```

The hook is `SessionStart`, `Stop`, `SubagentStop` or `PostToolUse`; the error is followed by
each of its causes, outermost first. The next session start reports the lines added since the last one,
once:

> Weigh-In: 2 hook error(s) since the last session, so capture may be off. Last: … (full log: …)

Markers raised while a hook was failing were not captured; `/weigh-in` sweeps the conversation
and catches what is still in it. `hook-errors.seen` beside the log only records how far it has
been reported. The log is never cleared by Weigh-In; truncate it yourself when you have read it.

## "Capture is off"

> Weigh-In: capture is off: this plugin has no weighin binary for Linux-aarch64; build one with
> scripts/build-plugin-bins.sh or install a release that ships it.

The launcher found no binary for this computer, so no hook records anything. In order:

1. "This copy has no binaries"? The plugin is the development tree: installed from `main`, a
   local checkout without a build, or a claude.ai organization sync of the `weigh-in`
   repository, whose default branch is `main` (B-18). Install from the `marketplace` branch
   (above), or let the organization sync `weigh-in-plugin` instead, which holds the releases
   (B-19); `main` holds no binaries.
2. A platform the release does not ship (Intel macOS, Linux on ARM, Windows on ARM)? Build one:
   `cargo install --path crates/weighin` from a checkout puts `weighin` on `PATH`, which the
   launcher falls back to.
3. Binary present but not running? The launcher sets the executable bit when it is missing;
   check the file under `~/.claude/plugins/cache/weigh-in/weigh/<version>/libexec/<target>/`
   and that nothing (antivirus, a `noexec` mount) blocks it.

In Cowork this message never appears: hooks there record nothing, and the sitting sweeps the
conversation instead (B-04).

## Windows: `ENOENT … plugin/libexec/weighin`

> Stop hook error: … ENOENT: no such file or directory, uv_spawn 'D:\repos\weigh-in\plugin/libexec/weighin'

On Windows, Claude Code starts `libexec/weighin` with no shell, and that path resolves to
`libexec/weighin.exe` (B-16). When that file is missing, every hook fails this way instead of
saying capture is off. Released packages ship it. A local checkout gets it only from the build
scripts of 0.5.0 or later, so a checkout built before 0.5.0 has the per-target build but not this
copy. Check with `dir <checkout>\plugin\libexec`. Rebuild in the checkout (not the repository
you were working in), then start a new session:

```powershell
cd <checkout>
pwsh scripts/build-plugin-bins.ps1
```

Or install from the `marketplace` branch (above), which ships `weighin.exe`, and remove the local
install.

A copy synced by claude.ai (its path has `…/rpm/plugin_…/` in it) with only `weighin` and
`weighin.cmd` in `libexec` came from the `weigh-in` repository's `main`, not from a release
(B-18): there `/weigh-in` says "this copy has no binaries", and every hook fails this way. Only
`weigh-in-plugin` should be synced; a copy from it has `libexec/weighin.exe` and
`libexec/x86_64-pc-windows-msvc/weighin.exe`.
