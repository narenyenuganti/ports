# portfwd — Lightweight SSH Port Forwarding TUI

## Overview

A single Rust binary that connects to a remote host via SSH, discovers listening ports, and lets you selectively forward them locally through an interactive terminal UI. Replaces Cursor's built-in port forwarding panel without requiring a full IDE.

## Usage

```
portfwd <ssh-config-host-alias>
```

Example: `portfwd my-remote`

## Architecture

Three layers in a single async binary:

```
┌─────────────────────────────────┐
│           TUI (ratatui)         │  Interactive table, keybinds
├─────────────────────────────────┤
│        Core / State Machine     │  Port list, forward lifecycle
├─────────────────────────────────┤
│      SSH Transport (russh)      │  Connection, discovery, tunnels
└─────────────────────────────────┘
```

### Flow

1. User runs `portfwd my-remote`
2. Tool parses `~/.ssh/config`, connects via `russh`, authenticates
3. Runs `ss -tlnp` on the remote via an exec channel, parses output
4. Presents discovered ports in a TUI table
5. User selects ports to forward with arrow keys + enter
6. Tool opens local TCP listener on the same port, proxies traffic through SSH channel
7. User can press `r` to refresh, `q` to quit

## SSH Connection & Config

### Config Parsing

Uses the `ssh2-config` crate to parse `~/.ssh/config`. Supports:
- `HostName`, `User`, `Port`, `IdentityFile`, and most common directives

Known limitations: `ProxyJump` and `Match` blocks may not be fully supported.

### Authentication Order

1. **SSH agent** (via `russh-keys` agent client) — tried first
2. **IdentityFile** from SSH config — read and use directly
3. **Default key paths** (`~/.ssh/id_ed25519`, `~/.ssh/id_rsa`)

### Connection Lifecycle

Single `russh` session for the entire tool lifetime. Port discovery and all forwarding channels multiplex over this one connection. If the connection drops, the TUI shows a disconnected state with an option to reconnect (`c` keybind).

## Port Discovery

### Mechanism

Executes `ss -tlnp` on the remote via an SSH exec channel. Parses output to extract listening ports.

### Filtering

Only includes ports bound to `0.0.0.0` or `::` — excludes `127.0.0.1`/`::1` (localhost-only services).

### Parsed Data Per Port

- Port number
- Bind address
- Process name and PID (when available — `ss -tlnp` only shows process info for the user's own processes)

### Refresh

Manual only, triggered by `r` keybind. Re-runs `ss -tlnp`, diffs against current state, updates the table. Ports that disappeared are marked stale. New ports appear as unforwarded.

### Fallback

If `ss` is unavailable on the remote, falls back to `netstat -tlnp` with the same parsing logic.

## Port Forwarding

### Mechanism

When the user selects a port to forward:

1. Opens a local TCP listener on `127.0.0.1:<remote_port>` (same port number by default)
2. On each incoming local connection, opens a `direct-tcpip` channel through the SSH session to the remote port (using the discovered bind address, e.g., `0.0.0.0:<remote_port>`)
3. Bidirectionally copies data between the local TCP stream and the SSH channel

### Port Conflicts

If the local port is already in use, the TUI shows an error inline and lets the user pick an alternate local port.

### Lifecycle

Each forward runs as a tokio task. Stopping a forward closes the local listener and all active connections for that port. Quitting the tool tears down all forwards and the SSH session cleanly.

### Concurrency

Multiple ports can be forwarded simultaneously. Each local listener and its connections are independent async tasks.

## TUI

### Framework

`ratatui` with `crossterm` backend.

### Layout

```
╔══════════════════════════════════════════════════════════════════╗
║  portfwd — my-remote (connected)                               ║
╠══════════════════════════════════════════════════════════════════╣
║  Status │ Port  │ Local Address    │ Process          │ PID     ║
║  ───────┼───────┼──────────────────┼──────────────────┼──────── ║
║  ●      │ 18080 │ localhost:18080  │ python3          │ 1234    ║
║  ○      │ 18120 │                  │ go-build/4d/...  │ 5678    ║
║  ○      │ 8080  │                  │ nginx            │ 910     ║
╠══════════════════════════════════════════════════════════════════╣
║  [enter] toggle forward  [r] refresh  [p] change port          ║
║  [c] reconnect  [q] quit                                       ║
╚══════════════════════════════════════════════════════════════════╝
```

### Key Elements

- `●` = actively forwarded (green), `○` = discovered but not forwarded
- Status bar at top shows host alias and connection state (connected / disconnected / reconnecting)

### Keybinds

| Key     | Action                                      |
|---------|---------------------------------------------|
| `↑`/`↓` | Navigate rows                              |
| `enter` | Toggle forwarding on/off for selected port  |
| `r`     | Rescan remote ports                         |
| `p`     | Prompt for alternate local port (inline input) |
| `c`     | Reconnect if SSH session dropped            |
| `q`     | Graceful shutdown — stop forwards, close SSH, exit |

## Project Structure

```
portfwd/
├── Cargo.toml
├── src/
│   ├── main.rs          # CLI entry point, arg parsing, app bootstrap
│   ├── ssh/
│   │   ├── mod.rs
│   │   ├── config.rs    # SSH config parsing
│   │   ├── connection.rs # russh session management, auth
│   │   └── discovery.rs # Remote port scanning (ss/netstat parsing)
│   ├── forward/
│   │   ├── mod.rs
│   │   └── tunnel.rs    # Local listener, bidirectional proxy via SSH channel
│   └── tui/
│       ├── mod.rs
│       ├── app.rs       # App state, event loop, keybind dispatch
│       ├── ui.rs        # ratatui layout and rendering
│       └── input.rs     # Key event handling, inline text input for port override
```

## Crate Dependencies

| Crate        | Purpose                              |
|--------------|--------------------------------------|
| `russh`      | SSH connection and channels          |
| `russh-keys` | Key loading and SSH agent            |
| `ssh2-config`| Parsing `~/.ssh/config`              |
| `ratatui`    | TUI rendering                        |
| `crossterm`  | Terminal backend for ratatui         |
| `tokio`      | Async runtime                        |
| `clap`       | CLI argument parsing                 |
| `anyhow`     | Error handling                       |

## Error Handling

- `anyhow` for top-level errors
- SSH and forwarding errors surface as inline TUI messages rather than crashing the app
- Connection failures show disconnected state with reconnect option
- Port conflicts prompt for alternate local port
