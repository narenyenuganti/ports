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

    parse_host_config(host_alias, &config_str)
}

/// Parse SSH config from a string (testable without filesystem).
pub fn parse_host_config(host_alias: &str, config_str: &str) -> Result<HostConfig> {
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

/// Enumerate the host aliases declared in an ssh config string.
///
/// Returns the aliases in file order. Multi-token `Host` lines are expanded
/// into one entry per token; tokens that are `*` or contain a glob (`*`/`?`)
/// are skipped. Duplicate aliases are removed while preserving first-seen
/// order.
pub fn parse_host_aliases(config_str: &str) -> Vec<String> {
    let mut aliases: Vec<String> = Vec::new();
    for line in config_str.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let (key, value) = match trimmed.split_once(char::is_whitespace) {
            Some((k, v)) => (k.to_lowercase(), v.trim()),
            None => continue,
        };
        if key != "host" {
            continue;
        }
        for token in value.split_whitespace() {
            if token == "*" || token.contains('*') || token.contains('?') {
                continue;
            }
            if !aliases.iter().any(|a| a == token) {
                aliases.push(token.to_string());
            }
        }
    }
    aliases
}

/// List the host aliases available in `~/.ssh/config`.
///
/// Returns an empty list (not an error) when the config file is absent.
pub fn list_host_aliases() -> Result<Vec<String>> {
    let path = dirs_ssh_config_path();
    match fs::read_to_string(&path) {
        Ok(config_str) => Ok(parse_host_aliases(&config_str)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => {
            Err(e).with_context(|| format!("Failed to read SSH config at {}", path.display()))
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    // ---- expand_tilde ----

    #[test]
    fn test_expand_tilde_with_tilde() {
        let path = PathBuf::from("~/.ssh/id_ed25519");
        let expanded = expand_tilde(&path);
        let home = dirs_home();
        assert_eq!(expanded, home.join(".ssh").join("id_ed25519"));
    }

    #[test]
    fn test_expand_tilde_without_tilde() {
        let path = PathBuf::from("/absolute/path/to/key");
        let expanded = expand_tilde(&path);
        assert_eq!(expanded, PathBuf::from("/absolute/path/to/key"));
    }

    #[test]
    fn test_expand_tilde_bare_tilde_no_slash() {
        // "~foo" should NOT expand (no slash after ~)
        let path = PathBuf::from("~foo");
        let expanded = expand_tilde(&path);
        assert_eq!(expanded, PathBuf::from("~foo"));
    }

    // ---- parse_host_config with full SSH config ----

    #[test]
    fn test_parse_known_host() {
        let config = "\
Host myserver
    HostName 10.0.0.5
    User ubuntu
    Port 2222
    IdentityFile ~/.ssh/mykey
";
        let result = parse_host_config("myserver", config).unwrap();
        assert_eq!(result.hostname, "10.0.0.5");
        assert_eq!(result.user, "ubuntu");
        assert_eq!(result.port, 2222);
        assert_eq!(result.identity_files.len(), 1);
        // The tilde should be expanded
        let home = dirs_home();
        assert_eq!(result.identity_files[0], home.join(".ssh").join("mykey"));
    }

    #[test]
    fn test_parse_unknown_host_uses_defaults() {
        let config = "\
Host myserver
    HostName 10.0.0.5
    User ubuntu
";
        let result = parse_host_config("unknown-host", config).unwrap();
        // hostname defaults to alias
        assert_eq!(result.hostname, "unknown-host");
        // user defaults to current user
        assert_eq!(result.user, whoami::username());
        // port defaults to 22
        assert_eq!(result.port, 22);
        assert!(result.identity_files.is_empty());
    }

    #[test]
    fn test_parse_host_with_wildcard() {
        let config = "\
Host *
    User defaultuser
    Port 22

Host prod-*
    User deploy
    Port 2222
";
        let result = parse_host_config("prod-web", config).unwrap();
        assert_eq!(result.user, "deploy");
        assert_eq!(result.port, 2222);
    }

    #[test]
    fn test_parse_empty_config() {
        let config = "";
        let result = parse_host_config("anything", config).unwrap();
        assert_eq!(result.hostname, "anything");
        assert_eq!(result.port, 22);
    }

    #[test]
    fn test_parse_identity_file() {
        let config = "\
Host withkey
    HostName 10.0.0.1
    IdentityFile ~/.ssh/mykey
";
        let result = parse_host_config("withkey", config).unwrap();
        assert!(!result.identity_files.is_empty());
        let home = dirs_home();
        assert_eq!(result.identity_files[0], home.join(".ssh").join("mykey"));
    }

    #[test]
    fn test_parse_absolute_identity_file() {
        let config = "\
Host abskey
    HostName 10.0.0.1
    IdentityFile /etc/ssh/special_key
";
        let result = parse_host_config("abskey", config).unwrap();
        assert!(!result.identity_files.is_empty());
        assert_eq!(
            result.identity_files[0],
            PathBuf::from("/etc/ssh/special_key")
        );
    }

    // ---- dirs_ssh_config_path ----

    #[test]
    fn test_ssh_config_path() {
        let path = dirs_ssh_config_path();
        assert!(path.ends_with(".ssh/config"));
    }

    // ---- parse_host_aliases ----

    #[test]
    fn parse_host_aliases_basic() {
        let aliases = parse_host_aliases("Host a\nHost b *.x\nHost *\n");
        assert_eq!(aliases, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn parse_host_aliases_skips_globs_and_dedupes() {
        let cfg = "\
# comment
Host alpha beta
    HostName 1.2.3.4
Host gamma ?eta *.glob
Host alpha
Host *
";
        let aliases = parse_host_aliases(cfg);
        assert_eq!(
            aliases,
            vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string(),]
        );
    }

    #[test]
    fn parse_host_aliases_empty_config() {
        assert!(parse_host_aliases("").is_empty());
    }
}
