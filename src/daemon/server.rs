#![deny(clippy::unwrap_used, clippy::expect_used)]
//! The Unix-domain-socket server: NDJSON framing and per-connection fan-out.
//!
//! A [`UnixListener`] accepts connections; each one is split into three tasks:
//!
//! - a **reader** that parses one [`Request`] per line (parse-don't-validate: a
//!   malformed line becomes a `BadRequest` ack rather than dropping the
//!   connection), forwards the [`crate::protocol::message::RequestBody`] to the
//!   actor and queues the resulting [`DaemonMessage::Ack`];
//! - a **state-pusher** that subscribes to the actor's `watch` and queues a
//!   [`DaemonMessage::State`] line whenever the snapshot changes (and once on
//!   connect);
//! - a single **writer** that owns the socket's write half and serializes every
//!   queued message as one JSON line. Funneling all writes through one task
//!   avoids sharing the write half between the reader and the pusher.
//!
//! Every task observes a child [`CancellationToken`] so the connection unwinds
//! cleanly on shutdown.

use std::path::Path;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::daemon::actor::ActorHandle;
use crate::protocol::error::ProtocolError;
use crate::protocol::message::{DaemonMessage, Request, StateSnapshot};

/// How many outbound messages a connection's writer queue can hold.
const WRITER_QUEUE_DEPTH: usize = 64;

/// Bind a [`UnixListener`] at `path`, removing any stale socket file first.
///
/// The caller is responsible for the single-instance check and for setting the
/// file permissions after bind.
pub fn bind(path: &Path) -> Result<UnixListener> {
    // A leftover socket file from a previous run blocks bind; remove it. The
    // single-instance guard (in `supervise`) has already confirmed no live
    // daemon is listening before we get here.
    if path.exists() {
        std::fs::remove_file(path)
            .with_context(|| format!("removing stale socket {}", path.display()))?;
    }
    UnixListener::bind(path).with_context(|| format!("binding socket {}", path.display()))
}

/// Accept connections on `listener` until `cancel` is triggered.
///
/// Each accepted connection is served by [`serve_connection`] on its own task
/// with a child cancellation token.
pub async fn accept_loop(
    listener: UnixListener,
    actor: ActorHandle,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            accepted = listener.accept() => {
                match accepted {
                    Ok((stream, _addr)) => {
                        let actor = actor.clone();
                        let conn_cancel = cancel.child_token();
                        tokio::spawn(serve_connection(stream, actor, conn_cancel));
                    }
                    Err(e) => log::warn!("accept failed: {e}"),
                }
            }
        }
    }
}

/// Serve a single connection: spawn reader, state-pusher, and writer tasks and
/// run until any of them ends or the token is cancelled.
pub async fn serve_connection(stream: UnixStream, actor: ActorHandle, cancel: CancellationToken) {
    let (read_half, write_half) = stream.into_split();
    let state_rx = actor.state.clone();
    let (out_tx, out_rx) = mpsc::channel::<DaemonMessage>(WRITER_QUEUE_DEPTH);

    let writer = tokio::spawn(writer_task(write_half, out_rx, cancel.clone()));
    let pusher = tokio::spawn(state_pusher_task(state_rx, out_tx.clone(), cancel.clone()));
    let reader = tokio::spawn(reader_task(read_half, actor, out_tx, cancel.clone()));

    // When any task finishes (EOF, error, or cancel), tear the rest down.
    tokio::select! {
        _ = writer => {}
        _ = pusher => {}
        _ = reader => {}
    }
    cancel.cancel();
}

/// Read NDJSON requests, dispatch them to the actor, and queue acks.
async fn reader_task(
    read_half: OwnedReadHalf,
    actor: ActorHandle,
    out_tx: mpsc::Sender<DaemonMessage>,
    cancel: CancellationToken,
) {
    let mut lines = BufReader::new(read_half).lines();
    loop {
        let line = tokio::select! {
            _ = cancel.cancelled() => break,
            next = lines.next_line() => match next {
                Ok(Some(line)) => line,
                Ok(None) => break, // EOF
                Err(e) => {
                    log::warn!("read error: {e}");
                    break;
                }
            },
        };

        if line.trim().is_empty() {
            continue;
        }

        let ack = handle_line(&line, &actor).await;
        if out_tx.send(ack).await.is_err() {
            break; // writer gone
        }
    }
}

/// Parse one request line and produce the matching ack.
///
/// Parse-don't-validate: a malformed line yields a `BadRequest` ack (id 0)
/// without dropping the connection.
async fn handle_line(line: &str, actor: &ActorHandle) -> DaemonMessage {
    match serde_json::from_str::<Request>(line) {
        Ok(req) => {
            let id = req.id;
            match actor.request(req.body).await {
                Ok(hosts) => DaemonMessage::Ack {
                    id,
                    error: None,
                    hosts,
                },
                Err(error) => DaemonMessage::Ack {
                    id,
                    error: Some(error),
                    hosts: None,
                },
            }
        }
        Err(e) => DaemonMessage::Ack {
            id: 0,
            error: Some(ProtocolError::BadRequest {
                detail: format!("malformed request: {e}"),
            }),
            hosts: None,
        },
    }
}

/// Subscribe to the state `watch` and queue a `State` line on every change,
/// starting with the current snapshot.
async fn state_pusher_task(
    mut state_rx: watch::Receiver<StateSnapshot>,
    out_tx: mpsc::Sender<DaemonMessage>,
    cancel: CancellationToken,
) {
    // Emit the current snapshot immediately so a freshly connected client sees
    // state without waiting for the next change.
    let initial = state_rx.borrow().clone();
    if out_tx.send(DaemonMessage::State(initial)).await.is_err() {
        return;
    }

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            changed = state_rx.changed() => {
                if changed.is_err() {
                    break; // actor's watch sender dropped
                }
                let snapshot = state_rx.borrow().clone();
                if out_tx.send(DaemonMessage::State(snapshot)).await.is_err() {
                    break;
                }
            }
        }
    }
}

/// Own the socket's write half and write every queued message as one JSON line.
async fn writer_task(
    mut write_half: OwnedWriteHalf,
    mut out_rx: mpsc::Receiver<DaemonMessage>,
    cancel: CancellationToken,
) {
    loop {
        let message = tokio::select! {
            _ = cancel.cancelled() => break,
            next = out_rx.recv() => match next {
                Some(message) => message,
                None => break, // all senders dropped
            },
        };
        if write_line(&mut write_half, &message).await.is_err() {
            break;
        }
    }
}

/// Serialize `message` as one JSON line and write it to `write_half`.
async fn write_line(write_half: &mut OwnedWriteHalf, message: &DaemonMessage) -> Result<()> {
    let mut json = serde_json::to_string(message).context("serializing daemon message")?;
    json.push('\n');
    write_half
        .write_all(json.as_bytes())
        .await
        .context("writing to socket")?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::daemon::actor::spawn;
    use crate::daemon::engine::MockEngine;
    use tokio::io::Lines;

    /// Read newline-delimited messages from `reader` until one satisfies
    /// `pred`, returning it. Fails the test on EOF.
    async fn read_until<R, F>(reader: &mut Lines<R>, pred: F) -> DaemonMessage
    where
        R: AsyncBufReadExt + Unpin,
        F: Fn(&DaemonMessage) -> bool,
    {
        loop {
            let line = reader
                .next_line()
                .await
                .unwrap()
                .expect("connection closed before expected message");
            let msg: DaemonMessage = serde_json::from_str(&line).unwrap();
            if pred(&msg) {
                return msg;
            }
        }
    }

    fn temp_socket_path() -> std::path::PathBuf {
        let unique = format!(
            "ports-test-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        std::env::temp_dir().join(unique)
    }

    #[tokio::test]
    async fn connect_yields_state_then_ack_and_bad_json_is_acked() {
        let path = temp_socket_path();
        let cancel = CancellationToken::new();
        let (actor, _join) = spawn(Box::new(MockEngine::with_ports(vec![])), cancel.clone());

        let listener = bind(&path).unwrap();
        let server = tokio::spawn(accept_loop(listener, actor, cancel.clone()));

        let client = UnixStream::connect(&path).await.unwrap();
        let (rd, mut wr) = client.into_split();
        let mut reader = BufReader::new(rd).lines();

        // The pusher emits the initial snapshot on connect.
        let state = read_until(&mut reader, |m| matches!(m, DaemonMessage::State(_))).await;
        assert!(matches!(state, DaemonMessage::State(_)));

        // A well-formed request gets an ack carrying its id.
        wr.write_all(b"{\"id\":1,\"type\":\"connect\"}\n")
            .await
            .unwrap();
        let ack = read_until(&mut reader, |m| matches!(m, DaemonMessage::Ack { .. })).await;
        match ack {
            DaemonMessage::Ack { id, .. } => assert_eq!(id, 1),
            other => panic!("expected ack, got {other:?}"),
        }

        // A malformed line yields a BadRequest ack and keeps the connection.
        wr.write_all(b"{bad json\n").await.unwrap();
        let bad = read_until(&mut reader, |m| {
            matches!(
                m,
                DaemonMessage::Ack {
                    error: Some(ProtocolError::BadRequest { .. }),
                    ..
                }
            )
        })
        .await;
        match bad {
            DaemonMessage::Ack { id, error, .. } => {
                assert_eq!(id, 0);
                assert!(matches!(error, Some(ProtocolError::BadRequest { .. })));
            }
            other => panic!("expected bad-request ack, got {other:?}"),
        }

        cancel.cancel();
        let _ = server.await;
        let _ = std::fs::remove_file(&path);
    }
}
