use anyhow::{Context, Result};
use async_trait::async_trait;
use russh::keys::key;
use russh::*;
use std::path::PathBuf;
use std::sync::Arc;

use super::config::HostConfig;

pub struct SshSession {
    pub handle: client::Handle<ClientHandler>,
}

pub struct ClientHandler;

#[async_trait]
impl client::Handler for ClientHandler {
    type Error = anyhow::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &key::PublicKey,
    ) -> Result<bool, Self::Error> {
        // Accept all host keys for now.
        // Future improvement: check known_hosts.
        Ok(true)
    }
}

impl SshSession {
    pub async fn connect(config: &HostConfig) -> Result<Self> {
        let ssh_config = client::Config {
            ..Default::default()
        };

        let handler = ClientHandler;

        let mut handle = client::connect(
            Arc::new(ssh_config),
            (config.hostname.as_str(), config.port),
            handler,
        )
        .await
        .with_context(|| format!("Failed to connect to {}:{}", config.hostname, config.port))?;

        let authenticated = try_agent_auth(&mut handle, &config.user).await
            || try_identity_files(&mut handle, &config.user, &config.identity_files).await
            || try_default_keys(&mut handle, &config.user).await;

        if !authenticated {
            anyhow::bail!(
                "Authentication failed for {}@{}:{}",
                config.user,
                config.hostname,
                config.port
            );
        }

        Ok(SshSession { handle })
    }

    /// Execute a command on the remote and return its stdout.
    pub async fn exec(&self, command: &str) -> Result<String> {
        let mut channel = self
            .handle
            .channel_open_session()
            .await
            .context("Failed to open SSH session channel")?;

        channel
            .exec(true, command)
            .await
            .context("Failed to execute remote command")?;

        let mut output = Vec::new();
        while let Some(msg) = channel.wait().await {
            match msg {
                ChannelMsg::Data { ref data } => {
                    output.extend_from_slice(data);
                }
                ChannelMsg::Eof => break,
                _ => {}
            }
        }

        String::from_utf8(output).context("Remote command output was not valid UTF-8")
    }

    /// Open a direct-tcpip channel for port forwarding.
    pub async fn open_direct_tcpip(
        &self,
        remote_host: &str,
        remote_port: u16,
        local_host: &str,
        local_port: u16,
    ) -> Result<Channel<client::Msg>> {
        self.handle
            .channel_open_direct_tcpip(
                remote_host,
                remote_port as u32,
                local_host,
                local_port as u32,
            )
            .await
            .context("Failed to open direct-tcpip channel")
    }
}

async fn try_agent_auth(handle: &mut client::Handle<ClientHandler>, user: &str) -> bool {
    let mut agent = match russh_keys::agent::client::AgentClient::connect_env().await {
        Ok(a) => a,
        Err(_) => return false,
    };

    let identities = match agent.request_identities().await {
        Ok(ids) => ids,
        Err(_) => return false,
    };

    // authenticate_future consumes the agent and returns it back in the tuple
    let mut current_agent = agent;
    for pubkey in identities {
        let (returned_agent, result) = handle
            .authenticate_future(user, pubkey, current_agent)
            .await;
        current_agent = returned_agent;
        if let Ok(true) = result {
            return true;
        }
    }
    false
}

async fn try_identity_files(
    handle: &mut client::Handle<ClientHandler>,
    user: &str,
    identity_files: &[PathBuf],
) -> bool {
    for path in identity_files {
        if try_key_file(handle, user, path).await {
            return true;
        }
    }
    false
}

async fn try_default_keys(handle: &mut client::Handle<ClientHandler>, user: &str) -> bool {
    let home = home::home_dir().expect("Could not determine home directory");
    let default_keys = [
        home.join(".ssh").join("id_ed25519"),
        home.join(".ssh").join("id_rsa"),
    ];
    for path in &default_keys {
        if try_key_file(handle, user, path).await {
            return true;
        }
    }
    false
}

async fn try_key_file(
    handle: &mut client::Handle<ClientHandler>,
    user: &str,
    path: &PathBuf,
) -> bool {
    let key = match russh_keys::load_secret_key(path, None) {
        Ok(k) => k,
        Err(_) => return false,
    };
    let key_pair = Arc::new(key);
    handle
        .authenticate_publickey(user, key_pair)
        .await
        .unwrap_or(false)
}
