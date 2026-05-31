# Ports

Ports is a macOS menu-bar app for SSH port forwarding. It discovers listening
ports on a remote host, forwards selected ports to localhost, and keeps the
original terminal UI available.

The app UI is Swift. SSH discovery and forwarding stay in the Rust `ports`
daemon, which the app controls over a local Unix socket.

## Features

- Discover listening ports on an SSH host.
- Start and stop forwards from the menu bar.
- Choose custom local ports.
- Open or copy forwarded localhost URLs.
- Send files to the connected host.
- Run the original keyboard-driven TUI from the terminal.

## Build

```sh
make app
```

The bundle is written to `build/Ports.app`, includes `assets/AppIcon.icns`, and
is ad-hoc signed.

## Install

```sh
rm -rf /Applications/Ports.app
ditto build/Ports.app /Applications/Ports.app
open /Applications/Ports.app
```

Because the app is ad-hoc signed, macOS may require right-clicking `Ports.app`
and choosing **Open** on first launch.

## CLI

```sh
cargo run -- <ssh-host-alias>
```

Use a host alias from `~/.ssh/config`.

## Verify

```sh
CARGO_TARGET_DIR="$HOME/.cache/ports-target" scripts/gate-fast.sh
scripts/smoke-app.sh
```

`gate-fast.sh` runs format, clippy, and Rust tests. `smoke-app.sh` verifies an
assembled app bundle.

## Development

See `AGENTS.md` for branch, commit, and merge-gate conventions.
