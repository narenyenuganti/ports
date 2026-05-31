#![deny(clippy::unwrap_used, clippy::expect_used)]
//! The [`Engine`] abstraction over the SSH core.
//!
//! The actor talks to an `Engine` rather than the SSH modules directly. This
//! lets the actor be unit-tested with a [`MockEngine`] that records calls and
//! returns scripted results, while the production `SshEngine` adapts the real
//! `ssh::connection`, `ssh::discovery`, `forward::tunnel`, and `forward::file`
//! modules.

use anyhow::{anyhow, Result};
use async_trait::async_trait;

use crate::ssh::config::HostConfig;
use crate::ssh::discovery::DiscoveredPort;

/// Abstraction over the SSH operations the daemon needs.
///
/// All port numbers crossing this boundary are plain `u16`; the protocol
/// `Port` newtype is mapped at the actor boundary, not here.
#[async_trait]
pub trait Engine: Send {
    /// Establish a connection to the host described by `cfg`.
    async fn connect(&mut self, cfg: &HostConfig) -> Result<()>;
    /// Discover the remote listening ports on the connected host.
    async fn discover(&self) -> Result<Vec<DiscoveredPort>>;
    /// Start forwarding `remote` to a local port.
    ///
    /// When `local` is `None` the OS chooses the local port. Returns the
    /// actual bound local port.
    async fn start_forward(&mut self, remote: u16, local: Option<u16>) -> Result<u16>;
    /// Stop forwarding the given remote port. A no-op if it is not forwarded.
    fn stop_forward(&mut self, remote: u16);
    /// Stop all active forwards.
    fn stop_all(&mut self);
    /// Send a local file to the host at the given remote path.
    async fn send_file(&self, local: &str, remote: &str) -> Result<()>;
}

// ---------------------------------------------------------------------------
// MockEngine
// ---------------------------------------------------------------------------

#[cfg(test)]
use std::collections::HashMap;

/// A scriptable, in-memory [`Engine`] for testing the actor.
///
/// It records the ports it should hand back from `discover`, the active
/// forwards (remote -> local), and exposes flags to inject failures.
#[cfg(test)]
#[derive(Default)]
pub struct MockEngine {
    /// Ports returned by [`Engine::discover`].
    pub ports: Vec<DiscoveredPort>,
    /// Active forwards keyed by remote port -> bound local port.
    pub forwards: HashMap<u16, u16>,
    /// When set, [`Engine::connect`] fails.
    pub fail_connect: bool,
    /// When set, [`Engine::discover`] fails.
    pub fail_discover: bool,
    /// When set, [`Engine::start_forward`] fails.
    pub fail_forward: bool,
    /// Whether [`Engine::connect`] has succeeded at least once.
    pub connected: bool,
    /// Number of successful connects (lets tests assert reconnection).
    pub connect_count: u32,
    /// Set when [`Engine::stop_all`] has been called.
    pub stop_all_called: bool,
    /// The next local port handed out when `start_forward` is given `None`.
    pub next_local: u16,
}

#[cfg(test)]
impl MockEngine {
    /// Construct a `MockEngine` scripted to discover `ports`.
    pub fn with_ports(ports: Vec<DiscoveredPort>) -> Self {
        Self {
            ports,
            next_local: 20000,
            ..Default::default()
        }
    }

    /// Helper to build a [`DiscoveredPort`] with sensible defaults.
    pub fn port(port: u16, process: Option<&str>, pid: Option<u32>) -> DiscoveredPort {
        DiscoveredPort {
            port,
            bind_address: "127.0.0.1".to_string(),
            process_name: process.map(str::to_string),
            pid,
        }
    }
}

#[cfg(test)]
#[async_trait]
impl Engine for MockEngine {
    async fn connect(&mut self, _cfg: &HostConfig) -> Result<()> {
        if self.fail_connect {
            return Err(anyhow!("mock connect failure"));
        }
        self.connected = true;
        self.connect_count += 1;
        Ok(())
    }

    async fn discover(&self) -> Result<Vec<DiscoveredPort>> {
        if !self.connected {
            return Err(anyhow!("not connected"));
        }
        if self.fail_discover {
            return Err(anyhow!("mock discover failure"));
        }
        Ok(self.ports.clone())
    }

    async fn start_forward(&mut self, remote: u16, local: Option<u16>) -> Result<u16> {
        if self.fail_forward {
            return Err(anyhow!("mock forward failure"));
        }
        let bound = match local {
            Some(0) | None => {
                let p = self.next_local;
                self.next_local = self.next_local.wrapping_add(1);
                p
            }
            Some(p) => p,
        };
        self.forwards.insert(remote, bound);
        Ok(bound)
    }

    fn stop_forward(&mut self, remote: u16) {
        self.forwards.remove(&remote);
    }

    fn stop_all(&mut self) {
        self.forwards.clear();
        self.stop_all_called = true;
    }

    async fn send_file(&self, _local: &str, _remote: &str) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn cfg() -> HostConfig {
        HostConfig {
            hostname: "example.com".to_string(),
            user: "me".to_string(),
            port: 22,
            identity_files: vec![],
        }
    }

    #[tokio::test]
    async fn mock_connect_then_discover_returns_scripted_ports() {
        let mut engine = MockEngine::with_ports(vec![
            MockEngine::port(5432, Some("postgres"), Some(11)),
            MockEngine::port(8080, None, None),
        ]);
        engine.connect(&cfg()).await.unwrap();
        let ports = engine.discover().await.unwrap();
        assert_eq!(ports.len(), 2);
        assert_eq!(ports[0].port, 5432);
        assert_eq!(ports[0].process_name.as_deref(), Some("postgres"));
        assert_eq!(ports[1].port, 8080);
    }

    #[tokio::test]
    async fn mock_discover_before_connect_errors() {
        let engine = MockEngine::with_ports(vec![]);
        assert!(engine.discover().await.is_err());
    }

    #[tokio::test]
    async fn mock_fail_connect_injects_error() {
        let mut engine = MockEngine {
            fail_connect: true,
            ..Default::default()
        };
        assert!(engine.connect(&cfg()).await.is_err());
        assert!(!engine.connected);
    }

    #[tokio::test]
    async fn mock_start_forward_records_and_returns_local() {
        let mut engine = MockEngine::with_ports(vec![]);
        engine.connect(&cfg()).await.unwrap();
        let bound = engine.start_forward(5432, None).await.unwrap();
        assert_eq!(engine.forwards.get(&5432), Some(&bound));
        // Explicit local port is honored.
        let bound2 = engine.start_forward(9000, Some(19000)).await.unwrap();
        assert_eq!(bound2, 19000);
        assert_eq!(engine.forwards.get(&9000), Some(&19000));
    }

    #[tokio::test]
    async fn mock_stop_forward_and_stop_all_mutate_map() {
        let mut engine = MockEngine::with_ports(vec![]);
        engine.connect(&cfg()).await.unwrap();
        engine.start_forward(5432, None).await.unwrap();
        engine.start_forward(8080, None).await.unwrap();
        engine.stop_forward(5432);
        assert!(!engine.forwards.contains_key(&5432));
        assert!(engine.forwards.contains_key(&8080));
        engine.stop_all();
        assert!(engine.forwards.is_empty());
        assert!(engine.stop_all_called);
    }

    #[tokio::test]
    async fn mock_fail_forward_injects_error() {
        let mut engine = MockEngine {
            fail_forward: true,
            ..Default::default()
        };
        engine.connect(&cfg()).await.unwrap();
        assert!(engine.start_forward(5432, None).await.is_err());
        assert!(engine.forwards.is_empty());
    }
}
