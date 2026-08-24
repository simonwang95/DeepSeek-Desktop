# DeepSeek Desktop

DeepSeek Desktop is an unofficial macOS-first desktop launcher and lifecycle
manager for a local DeepSeek Harness checkout. It is independent software and
has no affiliation with, authorization from, or endorsement by DeepSeek.

The project manages a user-owned Harness source directory and its local
process. It does not modify Harness product code, bundle Harness source into
the installer, or modify `~/.dsh`, LM Studio settings, credentials, or user
sessions.

## Status

The repository is maintained on the `dev` branch. The current implementation
provides a Tauri 2 + TypeScript UI and Rust application service with:

- first-run configuration and safe `git clone`;
- Git, Node.js, and pnpm detection with repair suggestions;
- Harness start, stop, restart, process-group tracking, orphan discovery,
  port checks, stdout/stderr streaming, and Web UI opening;
- a safe Harness update pipeline with dirty-worktree protection,
  optional explicit backup stash, fetch, fast-forward-only merge, dependency
  installation, configured build cleanup, rebuild, and conditional restart;
- LM Studio `/v1/models` detection, model display, and error reporting;
- automated domain tests, Rust tests, CI, and macOS `.app`/`.dmg` bundle
  build entry points.

The app deliberately does not pretend that code signing or notarization has
completed without the required Apple credentials.

## Requirements

- macOS Apple Silicon and Windows are supported targets; Linux remains covered
  in the process-management and path-validation design.
- Node.js 22.19 or newer in the 22 line, or Node.js 24 or newer, for the
  current Harness checkout.
- pnpm 11.7 or newer for the current Harness checkout.
- Git.
- Rust stable and the Tauri system prerequisites for local desktop builds.
- A local DeepSeek Harness checkout, or a Git URL that can be cloned.
- Optional: LM Studio running at `http://127.0.0.1:1234`.

## Development

```
git clone <this-private-repository> DeepSeek-Desktop
cd DeepSeek-Desktop
git switch dev
pnpm install
pnpm tauri:dev
```

The browser-only UI can be previewed with `pnpm dev`. It will show a clear
backend-unavailable message because process control is only available inside
Tauri.

For normal use, build the bundled desktop application and open the generated
package from the target OS. On macOS:

```
pnpm tauri:build
open "src-tauri/target/release/bundle/macos/DeepSeek Desktop.app"
```

This starts the desktop app without leaving a terminal window open. The
`pnpm tauri:dev` command is intentionally terminal-based because it also runs
the Vite development server. The build also produces a `.dmg` in the same
directory for installation or sharing. Without Apple signing and
notarization, macOS may require an explicit first-launch approval in System
Settings.

On Windows, `pnpm tauri build` produces the Windows installer formats selected
by Tauri. The target machine needs WebView2; current Windows versions normally
provide it, and the installer can bootstrap it when required. Development mode
still uses a terminal because it runs Vite.

## First setup

1. Open **设置**.
2. Choose a Harness source directory. The app never overwrites a non-empty
   directory.
3. Confirm the upstream Git URL, branch, and Web port.
4. Save settings.
5. Check the dependency cards. If Git, Node.js, or pnpm is missing or too old,
   click **安装系统依赖**. On macOS this uses Homebrew; on Windows it uses
   winget. The package manager must already be installed, and the operation is
   always started by an explicit button click.
6. If the checkout is absent, choose **首次安装**. The app invokes Git with an
   argument array, clones only into the configured target, and then automatically
   installs the locked Harness dependencies and builds the Web artifacts.
7. If an existing checkout has no build artifacts, **启动并自动准备** performs
   the same dependency installation and build before starting the service. The
   separate **安装依赖并构建** button remains available for a manual retry.

The default service command is equivalent to:

```
pnpm dsh web --no-open
```

The default Harness Web port is `3080`, matching the Harness README.

On macOS, a Finder-launched app does not inherit the interactive terminal
`PATH`. The desktop service now reads the login-shell PATH and searches common
NVM, Homebrew, Volta, and pnpm locations before running Git, Node, or pnpm. On
Windows it also recognizes `.exe`, `.cmd`, and `.bat` command wrappers and
common npm/pnpm installation directories. If your tools use a custom location,
enter the absolute executable path in **设置**. If an existing configuration
still says `main` while the remote has changed to `master`, update checks verify
the configured branch first and then fall back to the remote default branch
without rewriting your configuration.

## Safe lifecycle and update behavior

The app records its own configuration and runtime record in the platform
application-data directory. The runtime record contains the PID, command,
source path, port, and start time. On the next launch, it checks the saved PID
and command line before treating a process as an orphan; an unrelated reused
PID is not trusted.

Stopping sends an interrupt to the managed process group, waits for the port
to close, and then sends TERM if necessary. `kill -9` is not used as a normal
stop operation. If the process does not stop after the graceful sequence, the
app leaves it for manual inspection and does not continue an update.

An update follows this order:

1. inspect running state and stop the service;
2. confirm the port is released;
3. inspect the Git worktree;
4. abort on local changes unless the user explicitly chooses a backup stash;
5. fetch the configured remote;
6. merge only with `--ff-only`;
7. install locked dependencies;
8. remove only configured, checkout-relative build paths;
9. rebuild;
10. restart only after a successful build if the service was running before.

Any failure stops later steps, retains emitted logs, records the error, and
leaves the checkout and recovery stash available for inspection. No reset,
force checkout, recursive source deletion, or automatic stash pop is used.

## LM Studio

The default endpoint is:

```
http://127.0.0.1:1234/v1/models
```

The URL is configurable. The UI displays the returned model IDs and connection
errors. The current verification targets are
`qwen3.6-35b-a3b-nvfp4` and `qwen3.8-27b-nvfp4`; they are not an allowlist and
are not hard-coded as the only valid models. Requests do not include an API
key, and logs redact common authorization and secret fields.

## Checks

```
pnpm typecheck
pnpm test
cargo test --manifest-path src-tauri/Cargo.toml
pnpm build
pnpm tauri:build
```

`pnpm tauri:build` creates the macOS `.app` and `.dmg`. Use
`pnpm tauri:build:binary` only when a raw executable is needed for debugging or
CI. The neutral placeholder icon is not a DeepSeek official logo.

## Release path

The two update channels remain separate:

1. Harness updates use the configured Git remote and the local safe-update
   state machine.
2. DeepSeek Desktop updates are intended to use GitHub Releases for signed
   macOS installers and a future Tauri updater configuration.

Before publishing a desktop release, configure Apple Developer signing,
notarization credentials, updater signing keys, and private GitHub Actions
secrets. Add those secrets only in the release environment; never commit them.
The local bundle is currently unsigned; the CI compile job intentionally uses a
no-bundle build and does not claim to produce a signed installer.

## Repository boundary

`DeepSeek-Desktop` is its own Git repository. The managed
`deepseek-harness` checkout is an external user directory and must never
receive desktop source files. The app may read its README, Git metadata,
configuration, and command output, but it does not add files to the Harness
repository.

See [README.zh.md](README.zh.md), [CONTRIBUTING.md](CONTRIBUTING.md), and
[SECURITY.md](SECURITY.md).
