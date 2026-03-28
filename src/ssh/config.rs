use anyhow::{Context, Result};
use ssh2_config::{ParseRule, SshConfig};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct HostConfig {
    pub hostname: String,
    pub user: String,
    pub port: u16,
    pub identity_files: Vec<PathBuf>,
}

pub fn load_host_config(host_alias: &str) -> Result<HostConfig> {
    let config_path = dirs_ssh_config_path();
    let config_str = fs::read_to_string(&config_path)
        .with_context(|| format!("Failed to read SSH config at {}", config_path.display()))?;

    let config = SshConfig::default()
        .parse(&mut config_str.as_bytes(), ParseRule::STRICT)
        .context("Failed to parse SSH config")?;

    let params = config.query(host_alias);

    let hostname = params.host_name.unwrap_or_else(|| host_alias.to_string());

    let user = params.user.unwrap_or_else(|| whoami::username());

    let port = params.port.unwrap_or(22);

    let identity_files = params
        .identity_file
        .unwrap_or_default()
        .into_iter()
        .map(|p| expand_tilde(&p))
        .collect();

    Ok(HostConfig {
        hostname,
        user,
        port,
        identity_files,
    })
}

fn dirs_ssh_config_path() -> PathBuf {
    let home = dirs_home();
    home.join(".ssh").join("config")
}

fn dirs_home() -> PathBuf {
    home::home_dir().expect("Could not determine home directory")
}

fn expand_tilde(path: &PathBuf) -> PathBuf {
    let s = path.to_string_lossy();
    if s.starts_with("~/") {
        dirs_home().join(&s[2..])
    } else {
        path.clone()
    }
}
