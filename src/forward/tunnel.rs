use anyhow::{Context, Result};
use russh::ChannelMsg;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use crate::ssh::connection::SshSession;

pub struct ForwardManager {
    session: Arc<SshSession>,
    active_forwards: HashMap<u16, CancellationToken>,
}

impl ForwardManager {
    pub fn new(session: Arc<SshSession>) -> Self {
        Self {
            session,
            active_forwards: HashMap::new(),
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

        self.active_forwards.insert(remote_port, token);
        Ok(actual_port)
    }

    /// Stop forwarding a remote port.
    pub fn stop_forward(&mut self, remote_port: u16) {
        if let Some(token) = self.active_forwards.remove(&remote_port) {
            token.cancel();
        }
    }

    /// Stop all active forwards.
    pub fn stop_all(&mut self) {
        for (_, token) in self.active_forwards.drain() {
            token.cancel();
        }
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
