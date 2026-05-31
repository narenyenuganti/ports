use anyhow::{Context, Result};
use russh::ChannelMsg;
use std::path::Path;

use crate::ssh::connection::SshSession;

/// Send a local file to the remote machine over the SSH connection.
///
/// Opens an exec channel running `cat > '<remote_path>'`, writes the file
/// contents to stdin, then checks the exit status.
pub async fn send_file(session: &SshSession, local_path: &str, remote_path: &str) -> Result<()> {
    // Read local file
    let contents = tokio::fs::read(local_path)
        .await
        .with_context(|| format!("Failed to read local file '{}'", local_path))?;

    // Ensure remote parent directory exists, then write file
    let escaped_remote = remote_path.replace('\'', "'\\''");
    let escaped_dir = Path::new(remote_path)
        .parent()
        .unwrap_or(Path::new("/"))
        .to_string_lossy()
        .replace('\'', "'\\''");
    let command = format!("mkdir -p '{}' && cat > '{}'", escaped_dir, escaped_remote);

    let mut channel = session
        .handle
        .channel_open_session()
        .await
        .context("Failed to open SSH session channel for file transfer")?;

    channel
        .exec(true, command.as_bytes())
        .await
        .context("Failed to execute remote cat command")?;

    // Write file contents to channel stdin
    channel
        .data(&contents[..])
        .await
        .context("Failed to write file data to SSH channel")?;

    // Signal end of input
    channel
        .eof()
        .await
        .context("Failed to send EOF on SSH channel")?;

    // Wait for exit status
    let mut exit_status = None;
    while let Some(msg) = channel.wait().await {
        if let ChannelMsg::ExitStatus { exit_status: code } = msg {
            exit_status = Some(code);
        }
    }

    match exit_status {
        Some(0) => Ok(()),
        Some(code) => anyhow::bail!("Remote command exited with status {}", code),
        None => Ok(()), // No exit status received, assume success
    }
}
