# ports

A lightweight terminal UI for SSH port forwarding. Connect to a remote host, see what's listening, and forward ports — all from one interactive panel.

```
ports <host>
```

```
 ports — <host> (connected) [Remote]
┌ Remote Ports ────────────────────────────────────────────────┐
│ Status  Port   Local Address      Process          PID       │
│                                                              │
│ ●       7000   localhost:7000     python3          2770      │
│ ●       18080  localhost:18080    node             2155      │
│ ○       5990                      redis-server     6854      │
│ ○       39585                     nginx            6871      │
│ ○       50000                     redis-server     6854      │
└──────────────────────────────────────────────────────────────┘
 [enter] toggle  [o] open  [r] refresh  [p] change port
 [s] sort  [tab] local  [q] quit
```

## Features

- **Discover remote ports** — Scans the remote host via `ss`/`netstat` over SSH and lists all listening ports with process info
- **Discover local ports** — Press `Tab` to see what's listening on your local machine (`lsof` on macOS, `ss`/`netstat` on Linux)
- **Forward with one keypress** — Select a port and press `Enter` to tunnel it to localhost
- **Open in browser** — Press `o` to open `http://localhost:<port>` directly
- **Sort columns** — Press `s` to sort by any column (ascending/descending)
- **Custom local ports** — Press `p` to forward to a different local port if there's a conflict
- **SSH config aware** — Reads `~/.ssh/config` for hostnames, users, keys, and ports

## Install

```sh
cargo build --release
cp target/release/ports ~/.local/bin/
```

## Usage

```sh
# Connect using an SSH config host alias
ports <host>

# That's it. Everything else is in the TUI.
```

### Keybindings

| Key | Action |
|-----|--------|
| `Enter` | Toggle port forwarding on/off |
| `o` | Open port URL in browser |
| `r` | Refresh port scan |
| `p` | Set custom local port |
| `s` | Sort by column |
| `Tab` | Switch between Remote/Local views |
| `j`/`k` or `Up`/`Down` | Navigate |
| `q` | Quit |

## How it works

1. Parses `~/.ssh/config` for the given host alias
2. Connects via SSH (agent auth, then key files)
3. Runs `ss -tlnp` on the remote to discover listening ports
4. Renders an interactive table with [ratatui](https://github.com/ratatui/ratatui)
5. Forwards selected ports by opening local TCP listeners that proxy through SSH `direct-tcpip` channels
6. All tunnels multiplex over a single SSH connection

## Requirements

- Rust 1.70+
- SSH access to the remote host (agent or key-based auth)
- `~/.ssh/config` entry for the target host

## macOS menu bar app (Ports.app)

`Ports.app` is a native macOS menu-bar application (no Dock icon) that wraps the
`ports` functionality in a SwiftUI `MenuBarExtra`. It bundles the SwiftUI
front-end (`PortsBar`) together with the Rust daemon (`ports`); the front-end
launches the bundled daemon and talks to it over a newline-delimited JSON
Unix-domain socket under `~/Library/Application Support/com.ports.app/`.

### Build

```sh
make app
```

This builds the `ports` daemon as a universal binary (arm64 + x86_64; it falls
back to arm64-only with a clear warning if the x86_64 cross-build is
unavailable), builds the Swift front-end in release, assembles `build/Ports.app`,
and ad-hoc codesigns it.

Run the advisory post-build check:

```sh
./scripts/smoke-app.sh
```

It verifies the code signature (`codesign --verify --deep --strict`), pings the
bundled daemon over a Unix socket and asserts an ack, and confirms the bundle is
structured as a login item.

### Install

1. Run `make app`.
2. Drag `build/Ports.app` to `/Applications`.
3. First launch: right-click the app and choose **Open** (then **Open** again in
   the Gatekeeper dialog), since the app is ad-hoc signed and not notarized.
4. The `Ports` icon appears in the menu bar.
5. To start automatically, open the in-app **Settings** and turn on **Launch at
   Login** (registers via `SMAppService.mainApp`).

### Manual end-to-end checklist

Run through this after installing a fresh `Ports.app` against a real dev host:

- [ ] **Connect to a host** from Settings/the menu and confirm it shows connected.
- [ ] **Forward a port** and confirm the forward appears in the popover.
- [ ] **Open in Browser** opens the forwarded `http://localhost:<port>` URL.
- [ ] **Copy URL** puts the forwarded local URL on the clipboard.
- [ ] **Custom local port** — set a non-default local port and confirm the
      forward binds to it (the popover shows the actual bound port).
- [ ] **Send a file** via footer **Send file…** and confirm it arrives on the host.
- [ ] **Reconnect after a network drop** — drop the network, restore it, and
      confirm the daemon re-establishes the session/forwards.
- [ ] **Quit clears forwards** — choose **Quit** and confirm all forwards are
      torn down (local ports released).
- [ ] **Launch-at-login toggle** — toggle **Launch at Login** in Settings on and
      off; confirm the login item is added/removed (System Settings ▸ General ▸
      Login Items).

## Development

See [`AGENTS.md`](AGENTS.md) for the quality gate and conventions. Packaging a
release runs `make app` followed by `scripts/smoke-app.sh`; `scripts/gate-full.sh`
runs the smoke test automatically when `build/Ports.app` exists.
