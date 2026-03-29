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
