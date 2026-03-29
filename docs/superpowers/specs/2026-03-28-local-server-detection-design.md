# Local Server Detection — Design Spec

## Overview

Add a read-only "Local" view to portfwd's TUI that shows listening ports on the local machine. Users toggle between Remote and Local views with `Tab`. No reverse forwarding — purely informational.

## Local Discovery

### Module Location

New public function `discover_local_ports()` in `src/ssh/discovery.rs` (alongside `discover_remote_ports()`). Runs shell commands locally via `tokio::process::Command` instead of over SSH.

### Platform-Adaptive Strategy

At runtime, check `std::env::consts::OS`:

- **macOS** → `lsof -iTCP -sTCP:LISTEN -nP`
- **Linux** → `ss -tlnp 2>/dev/null`, fallback to `netstat -tlnp 2>/dev/null`

Linux reuses the existing `parse_ss_output()` and `parse_netstat_output()` parsers.

### New Parser: `parse_lsof_output()`

Parses macOS `lsof` output format:

```
COMMAND   PID  USER   FD  TYPE   DEVICE SIZE/OFF NODE NAME
node     1234  user   23u IPv4   0x...  0t0      TCP  127.0.0.1:3000 (LISTEN)
nginx     567  root    8u IPv6   0x...  0t0      TCP  [::]:80 (LISTEN)
```

Extracts per line:
- `COMMAND` → `process_name`
- `PID` → `pid`
- `NAME` column → parsed into `bind_address` and `port`

Produces `Vec<DiscoveredPort>` — same struct as remote discovery, no new types.

### Error Handling

If the discovery command fails or isn't found, `discover_local_ports()` returns an empty vec and the TUI shows "No local ports found" in the table area. A status message like "Local scan failed: lsof not found" appears in the status bar. This matches how remote discovery handles `ss`/`netstat` failures.

### Filtering

Include all LISTEN sockets (both wildcard and localhost-bound), same as remote discovery.

## App State Changes

### New Types

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewMode {
    Remote,
    Local,
}
```

### New Fields on `AppState`

| Field | Type | Purpose |
|-------|------|---------|
| `view_mode` | `ViewMode` | Current view, defaults to `Remote` |
| `local_ports` | `Vec<DiscoveredPort>` | Local listening ports (no `PortEntry` wrapper — no forwarding state) |
| `local_selected` | `usize` | Cursor position in local view |

Existing `ports`, `selected`, and all forwarding logic are untouched and only apply to the `Remote` view. Each view maintains its own cursor so switching feels seamless.

## Input Handling

### Tab Key

In normal mode, `KeyCode::Tab` toggles `state.view_mode` between `Remote` and `Local`. Pure state change, no new `Action` variant.

### Behavior by View Mode

| Key | Remote View | Local View |
|-----|------------|------------|
| `Tab` | Switch to Local | Switch to Remote |
| `Up`/`Down`/`j`/`k` | Move `selected` | Move `local_selected` |
| `Enter` | `ToggleForward` | No-op |
| `p` | Enter port input mode | No-op |
| `r` | Refresh remote ports | Refresh local ports |
| `c` | Reconnect | No-op |
| `q` | Quit | Quit |

### Refresh Action

The main loop checks `state.view_mode` to decide which discovery to run:
- `Remote` → `discover_remote_ports(&session)` (existing)
- `Local` → `discover_local_ports()`

## UI Changes

### Status Bar

Appends the active view label:

```
 portfwd — my-remote (connected) [Remote]
 portfwd — my-remote (connected) [Local]
```

### Port Table

**Remote view (existing):** No changes. Columns: Status, Port, Local Address, Process, PID. Block title: `" Remote Ports "`.

**Local view:** Simpler columns since there's no forwarding state:

| Bind Address | Port | Process | PID |
|---|---|---|---|

Block title: `" Local Ports "`. Selection highlight works the same way using `local_selected`.

### Help Bar

**Remote view:**
```
[enter] toggle  [r] refresh  [p] change port  [tab] local  [c] reconnect  [q] quit
```

**Local view:**
```
[tab] remote  [r] refresh  [q] quit
```

## Main Loop Integration

### Startup

After SSH connection, run both discoveries concurrently:
```rust
let (remote, local) = tokio::join!(
    discover_remote_ports(&session),
    discover_local_ports()
);
```

Populate `state.update_ports(remote?)` and `state.local_ports = local?`.

### Refresh

Check `state.view_mode`:
- `Remote` → re-run `discover_remote_ports`, call `state.update_ports()`
- `Local` → re-run `discover_local_ports`, assign to `state.local_ports`, clamp `local_selected`

## Testing

### `parse_lsof_output()` Unit Tests

Same pattern as existing parser tests:
- Normal multi-line output with various processes
- IPv4 and IPv6 entries
- Entries missing PID (permission denied)
- Empty output
- Malformed lines

### View Mode State Tests

- Tab toggles between `Remote` and `Local`
- `local_selected` is independent of `selected`
- `local_ports` updates don't affect `ports`
- `local_selected` clamped when `local_ports` shrinks

### Input Tests for Local View

- Navigation moves `local_selected`, not `selected`
- `Enter` and `p` are no-ops
- `r` returns `Refresh`
- `Tab` switches back to Remote

### Integration Test

One test that calls `discover_local_ports()` on the current machine and asserts it returns a non-empty `Vec<DiscoveredPort>` (the test runner itself has a listening port).

## Files Changed

| File | Change |
|------|--------|
| `src/ssh/discovery.rs` | Add `parse_lsof_output()`, `discover_local_ports()` |
| `src/tui/app.rs` | Add `ViewMode`, `local_ports`, `local_selected` fields and helpers |
| `src/tui/input.rs` | Handle `Tab`, view-mode-aware navigation and action gating |
| `src/tui/ui.rs` | View-mode-aware table rendering, status bar label, help bar variants |
| `src/main.rs` | Concurrent startup discovery, view-mode-aware refresh |
