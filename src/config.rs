//! TOML config load/save and path helpers (arch §8). Never stores plaintext credentials.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{CliError, Diag};
use crate::redaction;

pub const DEFAULT_BASE_URL: &str = "https://api.exa.ai";
pub const DEFAULT_ADMIN_BASE_URL: &str = "https://admin-api.exa.ai/team-management";
pub const DEFAULT_TIMEOUT: &str = "30s";
pub const DEFAULT_RETRY: u32 = 2;

/// Non-secret user config (D11/D12). Keys live in env/keyring only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default = "default_admin_base_url")]
    pub admin_base_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<u32>,
    #[serde(
        default,
        rename = "active_profile",
        skip_serializing_if = "Option::is_none"
    )]
    pub active_profile: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub profiles: BTreeMap<String, ProfileConfig>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(
        default,
        rename = "admin_base_url",
        skip_serializing_if = "Option::is_none"
    )]
    pub admin_base_url: Option<String>,
    #[serde(
        default,
        rename = "api_key_env",
        skip_serializing_if = "Option::is_none"
    )]
    pub api_key_env: Option<String>,
    #[serde(
        default,
        rename = "service_key_env",
        skip_serializing_if = "Option::is_none"
    )]
    pub service_key_env: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<u32>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            base_url: default_base_url(),
            admin_base_url: default_admin_base_url(),
            output: None,
            timeout: Some(DEFAULT_TIMEOUT.to_string()),
            retry: Some(DEFAULT_RETRY),
            active_profile: None,
            profiles: BTreeMap::new(),
        }
    }
}

fn default_base_url() -> String {
    DEFAULT_BASE_URL.to_string()
}

fn default_admin_base_url() -> String {
    DEFAULT_ADMIN_BASE_URL.to_string()
}

/// Resolve the config file path: `EXA_AGENT_CONFIG`, then XDG, then `~/.config/...`.
pub fn config_path() -> PathBuf {
    if let Ok(path) = std::env::var("EXA_AGENT_CONFIG") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.trim().is_empty() {
            return PathBuf::from(xdg).join("exa-agent-cli").join("config.toml");
        }
    }
    std::env::var("HOME")
        .map(|home| {
            PathBuf::from(home)
                .join(".config")
                .join("exa-agent-cli")
                .join("config.toml")
        })
        .unwrap_or_else(|_| PathBuf::from(".config/exa-agent-cli/config.toml"))
}

impl Config {
    pub fn load() -> Result<Self, CliError> {
        Self::load_from_path(&config_path())
    }

    pub fn load_from_path(path: &Path) -> Result<Self, CliError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = fs::read_to_string(path).map_err(|e| io_config_error(path, e))?;
        toml::from_str(&raw).map_err(|e| parse_config_error(path, e))
    }

    pub fn save(&self) -> Result<(), CliError> {
        self.save_to_path(&config_path())
    }

    pub fn save_to_path(&self, path: &Path) -> Result<(), CliError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| io_config_error(path, e))?;
        }
        let serialized = toml::to_string_pretty(self).map_err(|e| {
            CliError::Config(Diag::new(
                "config_invalid",
                format!("failed to serialize config: {e}"),
            ))
        })?;
        let tmp = path.with_extension("toml.tmp");
        {
            let mut file = fs::File::create(&tmp).map_err(|e| io_config_error(path, e))?;
            file.write_all(serialized.as_bytes())
                .map_err(|e| io_config_error(path, e))?;
            file.sync_all().map_err(|e| io_config_error(path, e))?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))
                .map_err(|e| io_config_error(path, e))?;
        }
        fs::rename(&tmp, path).map_err(|e| io_config_error(path, e))?;
        Ok(())
    }

    pub fn get_path(&self, path: &str) -> Result<Option<serde_json::Value>, CliError> {
        let segments = parse_path_segments(path)?;
        if segments.is_empty() {
            return Err(invalid_path(path));
        }
        match segments.len() {
            1 => match segments[0].as_str() {
                "base_url" => Ok(Some(json_string(&self.base_url))),
                "admin_base_url" => Ok(Some(json_string(&self.admin_base_url))),
                "output" => Ok(self.output.as_ref().map(|v| json_string(v))),
                "timeout" => Ok(self.timeout.as_ref().map(|v| json_string(v))),
                "retry" => Ok(self.retry.map(|n| serde_json::json!(n))),
                "active_profile" => Ok(self.active_profile.as_ref().map(|v| json_string(v))),
                _ => Err(invalid_path(path)),
            },
            2 if segments[0] == "profiles" => {
                let name = &segments[1];
                self.profiles
                    .get(name)
                    .map(profile_to_json)
                    .ok_or_else(|| unknown_profile(name))
                    .map(Some)
            }
            3 if segments[0] == "profiles" => {
                profile_field(self, &segments[1], &segments[2]).map(Some)
            }
            _ => Err(invalid_path(path)),
        }
    }

    pub fn set_path(&mut self, path: &str, value: &str) -> Result<(), CliError> {
        if is_forbidden_config_path(path) {
            return Err(CliError::Config(
                Diag::new(
                    "config_invalid",
                    "config must not store plaintext credentials; use env var names (e.g. api_key_env) or `exa-agent auth login`",
                )
                .with_suggestion("export EXA_API_KEY=…"),
            ));
        }
        let segments = parse_path_segments(path)?;
        match segments.len() {
            1 => match segments[0].as_str() {
                "base_url" => {
                    validate_https_url(value, "base_url")?;
                    self.base_url = value.to_string();
                }
                "admin_base_url" => {
                    validate_https_url(value, "admin_base_url")?;
                    self.admin_base_url = value.to_string();
                }
                "output" => self.output = Some(value.to_string()),
                "timeout" => self.timeout = Some(value.to_string()),
                "retry" => {
                    let n: u32 = value.parse().map_err(|_| invalid_value("retry", value))?;
                    self.retry = Some(n);
                }
                "active_profile" => {
                    if !self.profiles.contains_key(value) {
                        return Err(unknown_profile(value));
                    }
                    self.active_profile = Some(value.to_string());
                }
                _ => return Err(invalid_path(path)),
            },
            3 if segments[0] == "profiles" => {
                let name = segments[1].clone();
                let field = segments[2].clone();
                let profile = self.profiles.entry(name).or_default();
                set_profile_field(profile, &field, value)?;
            }
            _ => return Err(invalid_path(path)),
        }
        Ok(())
    }

    pub fn unset_path(&mut self, path: &str) -> Result<(), CliError> {
        let segments = parse_path_segments(path)?;
        match segments.len() {
            1 => match segments[0].as_str() {
                "base_url" => self.base_url = default_base_url(),
                "admin_base_url" => self.admin_base_url = default_admin_base_url(),
                "output" => self.output = None,
                "timeout" => self.timeout = None,
                "retry" => self.retry = None,
                "active_profile" => self.active_profile = None,
                _ => return Err(invalid_path(path)),
            },
            2 if segments[0] == "profiles" => {
                self.profiles.remove(&segments[1]);
            }
            3 if segments[0] == "profiles" => {
                let name = &segments[1];
                let field = &segments[2];
                let profile = self
                    .profiles
                    .get_mut(name)
                    .ok_or_else(|| unknown_profile(name))?;
                unset_profile_field(profile, field)?;
            }
            _ => return Err(invalid_path(path)),
        }
        Ok(())
    }

    pub fn list_json(&self) -> serde_json::Value {
        serde_json::json!({
            "path": config_path().display().to_string(),
            "baseUrl": self.base_url,
            "adminBaseUrl": self.admin_base_url,
            "output": self.output,
            "timeout": self.timeout,
            "retry": self.retry,
            "activeProfile": self.active_profile,
            "profiles": self.profiles.keys().collect::<Vec<_>>(),
        })
    }

    pub fn profiles_json(&self) -> serde_json::Value {
        let profiles: BTreeMap<String, serde_json::Value> = self
            .profiles
            .iter()
            .map(|(name, profile)| (name.clone(), profile_to_json(profile)))
            .collect();
        serde_json::json!({
            "activeProfile": self.active_profile,
            "profiles": profiles,
        })
    }

    pub fn create_profile(&mut self, name: &str) -> Result<(), CliError> {
        if name.is_empty() {
            return Err(invalid_value("profile name", name));
        }
        self.profiles.entry(name.to_string()).or_default();
        Ok(())
    }

    pub fn delete_profile(&mut self, name: &str) -> Result<(), CliError> {
        if self.profiles.remove(name).is_none() {
            return Err(unknown_profile(name));
        }
        if self.active_profile.as_deref() == Some(name) {
            self.active_profile = None;
        }
        Ok(())
    }

    pub fn use_profile(&mut self, name: &str) -> Result<(), CliError> {
        if !self.profiles.contains_key(name) {
            return Err(unknown_profile(name));
        }
        self.active_profile = Some(name.to_string());
        Ok(())
    }

    pub fn show_profile(&self, name: &str) -> Result<serde_json::Value, CliError> {
        self.profiles
            .get(name)
            .map(profile_to_json)
            .ok_or_else(|| unknown_profile(name))
    }

    pub fn effective_base_url(&self) -> &str {
        if let Some(profile) = self.active_profile() {
            if let Some(url) = profile.base_url.as_deref() {
                return url;
            }
        }
        &self.base_url
    }

    pub fn active_profile(&self) -> Option<&ProfileConfig> {
        self.active_profile
            .as_deref()
            .and_then(|name| self.profiles.get(name))
    }
}

fn profile_to_json(profile: &ProfileConfig) -> serde_json::Value {
    serde_json::json!({
        "baseUrl": profile.base_url,
        "adminBaseUrl": profile.admin_base_url,
        "apiKeyEnv": profile.api_key_env,
        "serviceKeyEnv": profile.service_key_env,
        "output": profile.output,
        "timeout": profile.timeout,
        "retry": profile.retry,
    })
}

fn profile_field(cfg: &Config, name: &str, field: &str) -> Result<serde_json::Value, CliError> {
    let profile = cfg
        .profiles
        .get(name)
        .ok_or_else(|| unknown_profile(name))?;
    match normalize_field(field).as_str() {
        "base_url" => Ok(profile
            .base_url
            .as_ref()
            .map(|v| json_string(v))
            .unwrap_or(serde_json::Value::Null)),
        "admin_base_url" => Ok(profile
            .admin_base_url
            .as_ref()
            .map(|v| json_string(v))
            .unwrap_or(serde_json::Value::Null)),
        "api_key_env" => Ok(profile
            .api_key_env
            .as_ref()
            .map(|v| json_string(v))
            .unwrap_or(serde_json::Value::Null)),
        "service_key_env" => Ok(profile
            .service_key_env
            .as_ref()
            .map(|v| json_string(v))
            .unwrap_or(serde_json::Value::Null)),
        "output" => Ok(profile
            .output
            .as_ref()
            .map(|v| json_string(v))
            .unwrap_or(serde_json::Value::Null)),
        "timeout" => Ok(profile
            .timeout
            .as_ref()
            .map(|v| json_string(v))
            .unwrap_or(serde_json::Value::Null)),
        "retry" => Ok(profile
            .retry
            .map(|n| serde_json::json!(n))
            .unwrap_or(serde_json::Value::Null)),
        _ => Err(invalid_path(&format!("profiles.{name}.{field}"))),
    }
}

fn set_profile_field(
    profile: &mut ProfileConfig,
    field: &str,
    value: &str,
) -> Result<(), CliError> {
    match normalize_field(field).as_str() {
        "base_url" => {
            validate_https_url(value, "base_url")?;
            profile.base_url = Some(value.to_string());
        }
        "admin_base_url" => {
            validate_https_url(value, "admin_base_url")?;
            profile.admin_base_url = Some(value.to_string());
        }
        "api_key_env" | "service_key_env" => {
            validate_env_var_name(value, &normalize_field(field))?;
            let target = match normalize_field(field).as_str() {
                "api_key_env" => &mut profile.api_key_env,
                "service_key_env" => &mut profile.service_key_env,
                _ => unreachable!(),
            };
            *target = Some(value.to_string());
        }
        "output" | "timeout" => {
            let target = match normalize_field(field).as_str() {
                "output" => &mut profile.output,
                "timeout" => &mut profile.timeout,
                _ => unreachable!(),
            };
            *target = Some(value.to_string());
        }
        "retry" => {
            let n: u32 = value.parse().map_err(|_| invalid_value("retry", value))?;
            profile.retry = Some(n);
        }
        _ => return Err(invalid_path(field)),
    }
    Ok(())
}

fn unset_profile_field(profile: &mut ProfileConfig, field: &str) -> Result<(), CliError> {
    match normalize_field(field).as_str() {
        "base_url" => profile.base_url = None,
        "admin_base_url" => profile.admin_base_url = None,
        "api_key_env" => profile.api_key_env = None,
        "service_key_env" => profile.service_key_env = None,
        "output" => profile.output = None,
        "timeout" => profile.timeout = None,
        "retry" => profile.retry = None,
        _ => return Err(invalid_path(field)),
    }
    Ok(())
}

fn parse_path_segments(path: &str) -> Result<Vec<String>, CliError> {
    if path.trim().is_empty() {
        return Err(invalid_path(path));
    }
    Ok(path
        .split('.')
        .map(|segment| segment.replace('-', "_"))
        .collect())
}

fn normalize_field(field: &str) -> String {
    field.replace('-', "_")
}

pub fn is_forbidden_config_path(path: &str) -> bool {
    let segments: Vec<&str> = path.split('.').collect();
    let Some(last) = segments.last() else {
        return false;
    };
    let normalized = last.replace('-', "_");
    if normalized.ends_with("_env") {
        return false;
    }
    redaction::is_secret_name(&normalized)
        || matches!(
            normalized.as_str(),
            "api_key" | "service_key" | "key" | "token" | "password"
        )
}

fn validate_https_url(value: &str, field: &str) -> Result<(), CliError> {
    if !is_valid_https_url(value) {
        return Err(CliError::Config(
            Diag::new(
                "config_invalid",
                format!("`{field}` must be an absolute https URL, got `{value}`"),
            )
            .with_suggestion(format!("exa-agent config set {field} https://api.exa.ai")),
        ));
    }
    Ok(())
}

pub fn is_valid_https_url(value: &str) -> bool {
    if !value.starts_with("https://")
        || value
            .chars()
            .any(|ch| ch.is_ascii_whitespace() || ch.is_control())
    {
        return false;
    }
    let rest = &value["https://".len()..];
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.is_empty() || authority.contains('@') {
        return false;
    }
    let host = authority
        .rsplit_once(':')
        .map(|(host, port)| {
            (!port.is_empty() && port.chars().all(|ch| ch.is_ascii_digit())).then_some(host)
        })
        .unwrap_or(Some(authority));
    let Some(host) = host else {
        return false;
    };
    !host.is_empty()
        && host.chars().any(|ch| ch.is_ascii_alphanumeric())
        && host
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '.'))
}

fn validate_env_var_name(value: &str, field: &str) -> Result<(), CliError> {
    if is_valid_env_var_name(value) && !crate::auth::looks_like_api_key(value) {
        return Ok(());
    }
    Err(CliError::Config(
        Diag::new(
            "config_invalid",
            format!("`{field}` must name an environment variable, not contain a credential"),
        )
        .with_suggestion(format!(
            "exa-agent config set profiles.<name>.{field} EXA_API_KEY"
        )),
    ))
}

fn is_valid_env_var_name(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn json_string(value: &str) -> serde_json::Value {
    serde_json::Value::String(value.to_string())
}

fn parse_config_error(path: &Path, err: toml::de::Error) -> CliError {
    CliError::Config(
        Diag::new(
            "config_parse_error",
            format!("failed to parse config at {}: {err}", path.display()),
        )
        .with_suggestion("exa-agent config path"),
    )
}

fn io_config_error(path: &Path, err: impl std::fmt::Display) -> CliError {
    CliError::Config(Diag::new(
        "config_invalid",
        format!("config I/O error at {}: {err}", path.display()),
    ))
}

fn invalid_path(path: &str) -> CliError {
    CliError::Config(
        Diag::new("config_invalid", format!("unknown config path `{path}`"))
            .with_suggestion("exa-agent config list"),
    )
}

fn invalid_value(field: &str, value: &str) -> CliError {
    CliError::Config(Diag::new(
        "config_invalid",
        format!("invalid value for `{field}`: `{value}`"),
    ))
}

fn unknown_profile(name: &str) -> CliError {
    CliError::Config(
        Diag::new(
            "unknown_profile",
            format!("profile `{name}` does not exist"),
        )
        .with_suggestion("exa-agent config profiles list"),
    )
}
