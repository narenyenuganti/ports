# Ports.app — macOS Menu Bar App Design

**Date:** 2026-05-30
**Status:** Approved (brainstorming)
**Topic:** Turn the `ports` SSH-forwarding TUI into a native macOS menu bar app, additively.

## Summary

Add a native macOS menu bar app (`Ports.app`) that lets the user see the listening
ports on a remote dev host and forward any of them to localhost with one click —
without opening a terminal. The app launches at login and lives only in the menu bar
(no Dock icon), in the form-factor spirit of CodexBar.

The work is **strictly feature-additive**. The existing Rust core and the ratatui TUI
are untouched. We add (1) a background daemon as a new subcommand on the same `ports`
binary and (2) a SwiftUI menu bar app that drives it.

## Goals

- One-click forwarding of a remote host's listening ports to localhost from the menu bar.
- No terminal, no `cd`, no `cargo run`. Launch at login; always ready.
- Reuse 100% of the existing Rust core (`ssh`, `forward`, `discovery`) unchanged.
- Keep the existing TUI and CLI working exactly as they do today.

## Non-Goals (v1 / YAGNI)

- Multiple simultaneous hosts (one active host at a time; switch in Settings).
- A local-ports view (remote ports only).
- Code signing notarization or Homebrew distribution (personal local build only).
- Deep per-forward health monitoring beyond "the local listener is alive."
- CodexBar's actual functionality (usage/limits/billing). We borrow only its form factor.

## Decisions (locked during brainstorming)

| Decision | Choice |
| --- | --- |
| Bridge | Rust **daemon + Unix socket** (JSON line protocol); SwiftUI app is a thin client |
| Hosts | **Single active host**, switchable in a **Settings** area (reads `~/.ssh/config`) |
| Ports shown | **Remote only** |
| v1 actions | forward toggle, **open in browser**, **copy URL**, **custom local port**, **file transfer**, launch-at-login |
| Form factor | **Rich SwiftUI popover** (`MenuBarExtra` `.window` style), icon with active-forward count badge |
| Daemon lifetime | **= app lifetime.** Quit stops the daemon and all forwards. Forwards survive the *popover* closing, not app quit. |
| Engine trait | **Yes** — introduce an `Engine` trait so the daemon's actor is testable without a live SSH host |
| Packaging | **Personal local build** — `Makefile` assembles an ad-hoc-signed `Ports.app`; drag to `/Applications` |
| Swift project | **SwiftPM executable** + `Makefile` bundling step; **no committed `.xcodeproj`** |

## Architecture

Three layers. Two front-ends (TUI and app) sit over one untouched core.

```
ports (TUI, unchanged)        Ports.app (new, SwiftUI MenuBarExtra, LSUIElement)
        │                              │  spawns/supervises + drives
        │                              ▼
        │              ┌─────────────────────────────────┐
        │              │ Unix domain socket (0600)        │
        │              │ newline-delimited JSON            │
        │              │  requests ↑   /   State ↓         │
        │              └─────────────────────────────────┘
        │                              │
        │                              ▼
        │                   ports daemon (new, Rust + tokio)
        │                   socket server → Actor (owns state) → Engine trait
        ▼                              │  reuses
┌─────────────────────────────────────▼─────────────────────────────────┐
│ Rust core (unchanged): ssh::config · ssh::connection (russh) ·          │
│                        ssh::discovery · forward::tunnel · forward::file │
└─────────────────────────────────────┬─────────────────────────────────┘
                                       ▼  SSH: exec(ss/netstat/lsof) + direct-tcpip
                               🖥 dev-desktop (remote host)
```

### Component responsibilities

- **Rust core** — unchanged. The only addition anywhere in `src/ssh` is a small
  `list_host_aliases()` helper (see below); everything else is consumed as-is.
- **`ports daemon`** — owns the active connection and all forwards, runs auto-refresh
  and auto-reconnect, and serves the socket. It is the single source of truth for
  runtime state. Stateless across restarts (the app re-supplies config on connect).
- **`Ports.app`** — owns persisted user preferences, spawns/supervises the daemon, and
  renders whatever `State` the daemon pushes. Performs open-browser and copy-URL itself.

## IPC Protocol

- **Transport:** Unix domain socket at
  `~/Library/Application Support/<bundle-id>/daemon.sock`, file mode `0600` (owner only).
- **Framing:** newline-delimited JSON, one message per line, UTF-8.
- **Types:** defined once in Rust with `serde` (`src/daemon/protocol.rs`); Swift mirrors
  them as hand-written `Codable` structs. The surface is intentionally small. The Rust
  definitions are the source of truth; any change updates both sides plus a protocol
  round-trip test.

### App → daemon (requests)

Each request carries a `u64` `id` for ack correlation.

| Request | Fields | Effect |
| --- | --- | --- |
| `SetConfig` | `host_alias: String`, `refresh_secs: u32`, `auto_reconnect: bool` | Set/replace active config (does not connect) |
| `Connect` | — | Connect to the active host and discover ports |
| `Disconnect` | — | Drop the session and stop all forwards |
| `Refresh` | — | Re-run remote discovery now |
| `StartForward` | `remote_port: u16`, `local_port: Option<u16>` | Bind locally and forward; `None` → default to `remote_port` |
| `StopForward` | `remote_port: u16` | Stop that forward |
| `SendFile` | `local_path: String`, `remote_path: Option<String>` | Send a file (default remote `/tmp/<filename>`) |
| `ListHosts` | — | Returns `~/.ssh/config` aliases (via `Ack` payload) |
| `Ping` | — | Liveness check (used during spawn/supervision) |
| `Shutdown` | — | Stop all forwards and exit the daemon |

### Daemon → app (messages)

- **`State` (authoritative snapshot, pushed on every change):**
  ```jsonc
  {
    "type": "State",
    "host": "dev-desktop",
    "status": "Connected",          // Disconnected | Connecting | Connected | Error
    "status_detail": null,          // error/reconnect message when relevant
    "ports": [
      { "remote_port": 3000, "process": "next", "pid": 1234,
        "forward": { "state": "Forwarding", "local_port": 3000 } },
      { "remote_port": 8080, "process": "api",  "pid": 5678,
        "forward": { "state": "Idle" } },
      { "remote_port": 5432, "process": "postgres", "pid": 90,
        "forward": { "state": "Error", "detail": "address in use" } }
    ]
  }
  ```
- **`Ack { id, ok: bool, error?: String, payload?: ... }`** — request result; `ListHosts`
  returns the alias list in `payload`.
- **`Event { ... }`** — transient notifications (e.g. file-transfer success/failure)
  that should surface as a toast but are not part of the persistent snapshot.

The app holds no derived runtime state; it renders the latest `State` and shows toasts
for `Event`s. Open-browser / copy-URL use the `local_port` already present in `State`.

## Daemon Internals

- **Actor model:** a single async task owns all mutable state
  (`active config`, `Option<connection>`, discovered ports, forward map). Inputs arrive
  on one mpsc channel from: (1) socket-connection reader tasks, (2) a periodic refresh
  timer, (3) engine completions. Outputs are `State` snapshots broadcast to all connected
  clients. This removes lock contention and makes ordering deterministic and testable.
- **`Engine` trait** abstracts the SSH/forward operations the actor needs:
  ```rust
  #[async_trait]
  trait Engine {
      async fn connect(&mut self, cfg: &HostConfig) -> Result<()>;
      async fn discover(&self) -> Result<Vec<DiscoveredPort>>;
      async fn start_forward(&mut self, remote: u16, local: Option<u16>) -> Result<u16>;
      fn stop_forward(&mut self, remote: u16);
      fn stop_all(&mut self);
      async fn send_file(&self, local: &str, remote: &str) -> Result<()>;
  }
  ```
  - **`SshEngine`** (production) wraps `SshSession` + `ForwardManager` + `send_file`.
  - **`MockEngine`** (tests) returns scripted results so actor behavior (connect →
    discover → snapshot, forward → snapshot, reconnect-on-failure) is unit-testable
    without a live host.
- **Reconnect:** mirrors the TUI's logic in `main.rs::run_loop` — on discovery/exec
  failure, attempt reconnect; on success, tear down existing forwards (set ports `Idle`),
  re-point the manager, re-discover, push updated `State`.
- **Auto-refresh:** timer at `refresh_secs`; also on `Connect` and explicit `Refresh`.

## Daemon Lifecycle & Supervision

1. **App launch:** connect to the socket and `Ping`. If no live daemon, spawn the
   bundled `ports daemon --socket <path>` (binary located via `Bundle.main`).
2. **Single instance:** the daemon refuses to start if the socket is already owned by a
   live daemon (bind/lock check); stale socket files are cleaned up on start.
3. **App quit:** send `Shutdown` (stops all forwards), then terminate the child if it
   does not exit promptly.
4. **Daemon crash:** the app detects socket EOF, shows a "reconnecting…" state, and
   respawns the daemon; on reconnect it re-issues `SetConfig` + `Connect`.
5. **Config ownership:** the app persists prefs (UserDefaults) and re-supplies them via
   `SetConfig` after every (re)spawn; the daemon keeps no config on disk.

## Settings & Host List

- **Host list:** add `pub fn list_host_aliases() -> Result<Vec<String>>` to
  `src/ssh/config.rs` (parse `Host` stanzas from `~/.ssh/config`, excluding pure
  wildcard `*` patterns). Exposed via the `ListHosts` request so **no SSH parsing lives
  in Swift**.
- **Settings (UserDefaults):** active host alias, launch-at-login, auto-refresh interval,
  open-browser-on-forward (default off), auto-reconnect (default on).

## Menu Bar UI

- `MenuBarExtra(...) { PopoverView() }.menuBarExtraStyle(.window)`.
- Icon: `arrow.left.arrow.right` template symbol; shows a count badge when forwards are
  active; dimmed/warning treatment when `status != Connected`.
- **Popover:** connection dot + active host + gear (Settings); one tile per remote port
  showing process name, `remote :p → localhost:p`, and a status pill. Selecting a tile
  reveals actions: Forward/Stop, **↗ Open**, **⧉ Copy URL**, and a **custom local-port**
  field. Footer: Refresh · Send file… · Settings · Quit.
- **File transfer:** footer "Send file…" → `NSOpenPanel` → prompt remote dir (default
  `/tmp`) → `SendFile` → toast on result.
- **Settings panel:** host picker, launch-at-login toggle, auto-refresh interval,
  open-browser-on-forward, auto-reconnect.

## Error Handling

| Failure | Surfaced as |
| --- | --- |
| Connect fails | `status: Error` + detail; red dot + Retry in popover |
| Local bind fails (port in use) | port `forward.state: Error`; error pill + nudge to set a custom local port |
| `send_file` fails | `Event` → toast with the error |
| Missing `~/.ssh/config` or unknown alias | surfaced in Settings (empty/disabled picker) |
| Daemon unreachable / crashed | app shows "reconnecting…", respawns daemon |

## Packaging (Personal Local Build)

- `Makefile` targets:
  - `cargo build --release` for `aarch64-apple-darwin` and `x86_64-apple-darwin`, then
    `lipo` into a universal `ports` binary.
  - SwiftPM build of the `PortsBar` executable target.
  - Assemble `Ports.app`: generate `Info.plist` (`LSUIElement = true`, bundle id, version),
    place the Swift executable in `Contents/MacOS/`, place the universal `ports` binary in
    `Contents/Resources/`.
  - Ad-hoc codesign: `codesign --force --deep -s - Ports.app`.
- Install: drag `Ports.app` to `/Applications`; first launch via right-click → Open
  (Gatekeeper, one time). Launch-at-login via the in-app SMAppService toggle.

## Repository Layout (monorepo, additive)

```
Cargo.toml
src/
  lib.rs                 # unchanged
  main.rs                # + `daemon` subcommand (clap)
  ssh/config.rs          # + list_host_aliases()
  ssh/...                # unchanged
  forward/...            # unchanged
  tui/...                # unchanged
  daemon/                # NEW
    mod.rs
    protocol.rs          # serde request/response/event types (source of truth)
    server.rs            # unix socket accept loop, framing, client fan-out
    actor.rs             # state-owning task
    engine.rs            # Engine trait + SshEngine + MockEngine
app/                     # NEW (SwiftUI menu bar app)
  Package.swift
  Sources/PortsBar/
    PortsBarApp.swift     # @main, MenuBarExtra
    DaemonClient.swift    # socket connect, spawn/supervise, encode/decode
    Protocol.swift        # Codable mirror of protocol.rs
    AppModel.swift        # ObservableObject; holds latest State + prefs
    PopoverView.swift
    SettingsView.swift
Makefile                 # build rust + swift, assemble Ports.app
docs/superpowers/specs/2026-05-30-macos-menu-bar-app-design.md
```

## Testing Strategy

- **Rust**
  - Protocol `serde` round-trip tests (every request and message variant).
  - Actor tests driven by `MockEngine`: connect → `State(Connected, ports)`,
    `StartForward` → snapshot with `Forwarding`, discovery failure → reconnect path,
    `Disconnect`/`Shutdown` → forwards cleared.
  - `list_host_aliases()` parser tests (mirroring existing `config.rs` test style).
  - Existing core/discovery/tracker tests remain green.
- **Swift**
  - `Codable` decode tests against sample JSON snapshots (from the Rust round-trip fixtures).
  - `AppModel` state-mapping tests (snapshot → rendered view state, badge count, statuses).
- **Manual**
  - End-to-end against a real dev host: connect, forward, open browser, copy URL, custom
    port, send file, reconnect after dropping the network, quit-clears-forwards,
    launch-at-login.

## Risks & Mitigations

- **Protocol drift (Rust ↔ Swift):** small surface; shared fixtures; round-trip tests on
  both sides. If it becomes painful, adopt `typeshare` codegen later.
- **Gatekeeper friction (ad-hoc signed):** documented one-time right-click → Open; revisit
  notarization only if distribution is ever wanted.
- **Forward health depth:** v1 treats "listener alive" as forwarding. Connection-level
  death surfaces on the next refresh/reconnect. Deeper health is deferred.

## Open Questions

None blocking. Deferred items are listed under Non-Goals.
