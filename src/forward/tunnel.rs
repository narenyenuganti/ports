use anyhow::{Context, Result};
use russh::ChannelMsg;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use crate::ssh::connection::SshSession;

/// Tracks active port forwards and their cancellation tokens.
/// Separated from ForwardManager to enable independent testing.
pub struct ForwardTracker {
    active: HashMap<u16, CancellationToken>,
}

impl ForwardTracker {
    pub fn new() -> Self {
        Self {
            active: HashMap::new(),
        }
    }

    pub fn insert(&mut self, remote_port: u16, token: CancellationToken) {
        self.active.insert(remote_port, token);
    }

    pub fn stop(&mut self, remote_port: u16) -> bool {
        if let Some(token) = self.active.remove(&remote_port) {
            token.cancel();
            true
        } else {
            false
        }
    }

    pub fn stop_all(&mut self) {
        for (_, token) in self.active.drain() {
            token.cancel();
        }
    }

    #[allow(dead_code)]
    pub fn is_forwarding(&self, remote_port: u16) -> bool {
        self.active.contains_key(&remote_port)
    }

    #[allow(dead_code)]
    pub fn count(&self) -> usize {
        self.active.len()
    }
}

pub struct ForwardManager {
    session: Arc<SshSession>,
    tracker: ForwardTracker,
}

impl ForwardManager {
    pub fn new(session: Arc<SshSession>) -> Self {
        Self {
            session,
            tracker: ForwardTracker::new(),
        }
    }

    /// Start forwarding: bind locally and proxy to remote via SSH.
    /// Returns the actual local port bound.
    pub async fn start_forward(
        &mut self,
        remote_host: &str,
        remote_port: u16,
        local_port: u16,
    ) -> Result<u16> {
        let listener = TcpListener::bind(("127.0.0.1", local_port))
            .await
            .with_context(|| format!("Failed to bind local port {}", local_port))?;

        let actual_port = listener.local_addr()?.port();
        let token = CancellationToken::new();
        let child_token = token.clone();
        let session = self.session.clone();
        let r_host = remote_host.to_string();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = child_token.cancelled() => break,
                    accept = listener.accept() => {
                        match accept {
                            Ok((tcp_stream, _)) => {
                                let session = session.clone();
                                let r_host = r_host.clone();
                                let conn_token = child_token.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = handle_connection(
                                        tcp_stream,
                                        &session,
                                        &r_host,
                                        remote_port,
                                        actual_port,
                                        conn_token,
                                    ).await {
                                        log::warn!("Forward connection error: {}", e);
                                    }
                                });
                            }
                            Err(e) => {
                                log::warn!("Accept error on port {}: {}", actual_port, e);
                            }
                        }
                    }
                }
            }
        });

        self.tracker.insert(remote_port, token);
        Ok(actual_port)
    }

    /// Stop forwarding a remote port.
    pub fn stop_forward(&mut self, remote_port: u16) {
        self.tracker.stop(remote_port);
    }

    /// Stop all active forwards.
    pub fn stop_all(&mut self) {
        self.tracker.stop_all();
    }
}

async fn handle_connection(
    mut tcp_stream: tokio::net::TcpStream,
    session: &SshSession,
    remote_host: &str,
    remote_port: u16,
    local_port: u16,
    token: CancellationToken,
) -> Result<()> {
    let mut channel = session
        .open_direct_tcpip(remote_host, remote_port, "127.0.0.1", local_port)
        .await?;

    let (mut tcp_read, mut tcp_write) = tcp_stream.split();
    let mut buf_from_tcp = vec![0u8; 8192];

    loop {
        tokio::select! {
            _ = token.cancelled() => break,
            result = tcp_read.read(&mut buf_from_tcp) => {
                match result {
                    Ok(0) => break,
                    Ok(n) => {
                        channel.data(&buf_from_tcp[..n]).await?;
                    }
                    Err(e) => return Err(e.into()),
                }
            }
            msg = channel.wait() => {
                match msg {
                    Some(ChannelMsg::Data { ref data }) => {
                        tcp_write.write_all(data).await?;
                    }
                    Some(ChannelMsg::Eof) | None => break,
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    // ---- ForwardTracker tests ----

    #[test]
    fn test_tracker_new_is_empty() {
        let tracker = ForwardTracker::new();
        assert_eq!(tracker.count(), 0);
        assert!(!tracker.is_forwarding(8080));
    }

    #[test]
    fn test_tracker_insert_and_query() {
        let mut tracker = ForwardTracker::new();
        let token = CancellationToken::new();
        tracker.insert(8080, token);
        assert!(tracker.is_forwarding(8080));
        assert!(!tracker.is_forwarding(3000));
        assert_eq!(tracker.count(), 1);
    }

    #[test]
    fn test_tracker_stop_existing() {
        let mut tracker = ForwardTracker::new();
        let token = CancellationToken::new();
        let child = token.clone();
        tracker.insert(8080, token);

        let removed = tracker.stop(8080);
        assert!(removed);
        assert!(!tracker.is_forwarding(8080));
        assert_eq!(tracker.count(), 0);
        assert!(child.is_cancelled());
    }

    #[test]
    fn test_tracker_stop_nonexistent() {
        let mut tracker = ForwardTracker::new();
        let removed = tracker.stop(9999);
        assert!(!removed);
    }

    #[test]
    fn test_tracker_stop_all() {
        let mut tracker = ForwardTracker::new();
        let t1 = CancellationToken::new();
        let t2 = CancellationToken::new();
        let c1 = t1.clone();
        let c2 = t2.clone();
        tracker.insert(8080, t1);
        tracker.insert(3000, t2);
        assert_eq!(tracker.count(), 2);

        tracker.stop_all();
        assert_eq!(tracker.count(), 0);
        assert!(c1.is_cancelled());
        assert!(c2.is_cancelled());
    }

    #[test]
    fn test_tracker_stop_all_empty() {
        let mut tracker = ForwardTracker::new();
        tracker.stop_all(); // should not panic
        assert_eq!(tracker.count(), 0);
    }

    #[test]
    fn test_tracker_insert_overwrites() {
        let mut tracker = ForwardTracker::new();
        let t1 = CancellationToken::new();
        let c1 = t1.clone();
        tracker.insert(8080, t1);

        let t2 = CancellationToken::new();
        tracker.insert(8080, t2);
        assert_eq!(tracker.count(), 1);
        // Old token is NOT auto-cancelled (the HashMap just drops it)
        assert!(!c1.is_cancelled());
    }

    #[test]
    fn test_tracker_multiple_ports() {
        let mut tracker = ForwardTracker::new();
        tracker.insert(8080, CancellationToken::new());
        tracker.insert(3000, CancellationToken::new());
        tracker.insert(5000, CancellationToken::new());
        assert_eq!(tracker.count(), 3);

        tracker.stop(3000);
        assert_eq!(tracker.count(), 2);
        assert!(tracker.is_forwarding(8080));
        assert!(!tracker.is_forwarding(3000));
        assert!(tracker.is_forwarding(5000));
    }

    // ---- Port binding tests ----

    #[tokio::test]
    async fn test_bind_port_zero_gets_os_port() {
        let listener = TcpListener::bind(("127.0.0.1", 0u16)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(port > 0);
    }

    #[tokio::test]
    async fn test_bind_port_conflict() {
        // Bind a port
        let listener = TcpListener::bind(("127.0.0.1", 0u16)).await.unwrap();
        let port = listener.local_addr().unwrap().port();

        // Try to bind the same port — should fail
        let result = TcpListener::bind(("127.0.0.1", port)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_bind_releases_on_drop() {
        let port;
        {
            let listener = TcpListener::bind(("127.0.0.1", 0u16)).await.unwrap();
            port = listener.local_addr().unwrap().port();
            // listener drops here
        }

        // Should be able to rebind after drop
        let result = TcpListener::bind(("127.0.0.1", port)).await;
        assert!(result.is_ok());
    }
}
