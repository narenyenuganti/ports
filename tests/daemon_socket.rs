//! Integration tests for the daemon's supervision and socket lifecycle.
//!
//! These exercise the public `ports::daemon` surface without a live SSH host:
//! the default socket-path builder and the single-instance refuse path of
//! `run()` (which returns `Ok(())` instead of binding when a live daemon is
//! already present). Full request/response framing is covered by the in-crate
//! `daemon::server` unit test, which can use the `#[cfg(test)]` `MockEngine`.

use std::path::PathBuf;
use std::time::Duration;

use ports::daemon::supervise::{check_single_instance, default_socket_path, InstanceCheck};

fn temp_socket_path() -> PathBuf {
    let unique = format!(
        "ports-it-{}-{}.sock",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    std::env::temp_dir().join(unique)
}

#[test]
fn default_socket_path_is_under_app_support() {
    let path = default_socket_path();
    let s = path.to_string_lossy();
    assert!(
        s.ends_with("Library/Application Support/com.ports.app/daemon.sock"),
        "unexpected default socket path: {s}"
    );
}

#[tokio::test]
async fn single_instance_check_is_safe_when_no_socket() {
    let path = temp_socket_path();
    let check = check_single_instance(&path)
        .await
        .expect("single-instance check should not error");
    assert_eq!(check, InstanceCheck::SafeToBind);
}

#[tokio::test]
async fn run_refuses_when_a_live_daemon_already_owns_the_socket() {
    let path = temp_socket_path();

    // First daemon: run with the production engine (SshEngine). It binds the
    // socket and serves it; it never needs to connect to a host for this test.
    let daemon_path = path.clone();
    let first = tokio::spawn(async move { ports::daemon::run(Some(daemon_path)).await });

    // Wait until the socket is live (answers a Ping).
    let mut alive = false;
    for _ in 0..50 {
        if ports::daemon::supervise::is_daemon_alive(&path).await {
            alive = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(alive, "first daemon did not come up on {}", path.display());

    // Second `run()` with the same socket must detect the live daemon and
    // return Ok without binding (single-instance guard).
    let second = ports::daemon::run(Some(path.clone())).await;
    assert!(second.is_ok(), "second run should return Ok: {second:?}");

    // Tear the first daemon down by sending Shutdown over the socket.
    {
        use tokio::io::AsyncWriteExt;
        use tokio::net::UnixStream;
        if let Ok(mut stream) = UnixStream::connect(&path).await {
            let _ = stream
                .write_all(b"{\"id\":2,\"type\":\"shutdown\"}\n")
                .await;
        }
    }

    // The first daemon should exit cleanly.
    let _ = tokio::time::timeout(Duration::from_secs(5), first).await;
    let _ = std::fs::remove_file(&path);
}
