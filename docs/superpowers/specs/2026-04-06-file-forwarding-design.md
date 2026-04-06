# File Forwarding Design

## Overview

Add the ability to forward (send) a local file to a remote machine over the existing SSH connection. By default, the remote destination is `/tmp/<local-path>`. Available as both a TUI keybinding and a CLI subcommand.

## Architecture

### New module: `src/forward/file.rs`

Handles streaming a local file over an SSH exec channel (`cat > '/remote/path'`) to the remote host. Reuses the existing `SshSession` connection.

### New input modes

- `InputMode::FilePathInput(String)` — user types the local file path
- `InputMode::RemotePathInput { local: String, remote: String }` — user edits the pre-filled remote destination path

### New action

- `Action::SendFile { local: String, remote: String }` — triggers the async file transfer from the main loop

### New CLI subcommand

- `ports send-file <host> <local-path> [remote-path]` — runs the transfer without entering the TUI

### Keybinding

- `[f]` in remote normal mode only

## TUI Flow

1. User presses `[f]` in remote view normal mode
2. Status bar shows prompt: `Local file path: ` with text input (same style as port input)
3. User types local file path, presses Enter
4. App validates the local file exists. If not, shows error in status bar, returns to normal mode
5. Status bar shows prompt: `Remote path: /tmp/<local-path>` pre-filled and editable, cursor at end
6. User edits or accepts, presses Enter
7. Status bar shows spinner: "Sending `<filename>`..."
8. On success: status bar shows "Sent `<filename>` to `<remote-path>`" for a few seconds
9. On failure: status bar shows error message
10. Esc at any point cancels and returns to normal mode

Help bar and help overlay updated to show `[f] send file` in the remote view.

## File Transfer Mechanism

The transfer function in `forward/file.rs`:

1. Read local file into memory
2. Open a new SSH session channel on the existing connection
3. Execute `cat > '<remote_path>'` on the remote with proper shell escaping
4. Write the file contents to the channel's stdin
5. Send EOF on the channel
6. Wait for the channel to close and check exit status
7. Return success/failure

### Error cases

- Local file doesn't exist — caught before transfer starts (TUI step 4)
- Local file not readable (permission denied) — caught before transfer starts
- Remote write fails (disk full, permission denied) — detected via non-zero exit status
- SSH channel error (connection lost) — propagated as error

No size limit enforced. If the file fits in memory and the remote has space, it transfers.

## CLI Subcommand

`ports send-file <host> <local-path> [remote-path]`

- Parses SSH config for the host (same as TUI entry point)
- Establishes SSH connection
- Validates local file exists
- Defaults remote path to `/tmp/<local-path>` if not provided
- Runs the transfer with a terminal spinner (no TUI)
- Prints success/failure message and exits
- Uses the same `forward/file.rs` transfer function as the TUI

## Testing

### Unit tests in `input.rs`

- `[f]` in remote normal mode enters `FilePathInput` mode
- `[f]` in local view does nothing
- Enter in `FilePathInput` transitions to `RemotePathInput` with correct default remote path
- Enter in `RemotePathInput` produces `Action::SendFile` with correct paths
- Esc in both input modes returns to normal mode
- Character input and backspace work correctly in both modes

### Integration tests

- Transfer function requires an SSH connection; manual or tested against a local SSH server

### Rendering

- No new tests for UI rendering, following existing project conventions
