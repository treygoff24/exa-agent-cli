//! Config module tests (Wave 1C).

use exa_agent_cli::config::{self, Config, DEFAULT_BASE_URL};
use exa_agent_cli::error::CliError;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn temp_config_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "exa-agent-config-test-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir.join("config.toml")
}

struct EnvGuard {
    key: &'static str,
    previous: Option<String>,
    _lock: MutexGuard<'static, ()>,
}

impl EnvGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let lock = ENV_LOCK.lock().unwrap();
        let previous = std::env::var(key).ok();
        unsafe { std::env::set_var(key, value) };
        Self {
            key,
            previous,
            _lock: lock,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => unsafe { std::env::set_var(self.key, value) },
            None => unsafe { std::env::remove_var(self.key) },
        }
    }
}

struct MultiEnvGuard {
    previous: Vec<(&'static str, Option<String>)>,
    _lock: MutexGuard<'static, ()>,
}

impl MultiEnvGuard {
    fn set(values: &[(&'static str, Option<&str>)]) -> Self {
        let lock = ENV_LOCK.lock().unwrap();
        let previous = values
            .iter()
            .map(|(key, _)| (*key, std::env::var(key).ok()))
            .collect();
        for (key, value) in values {
            match value {
                Some(value) => unsafe { std::env::set_var(key, value) },
                None => unsafe { std::env::remove_var(key) },
            }
        }
        Self {
            previous,
            _lock: lock,
        }
    }
}

impl Drop for MultiEnvGuard {
    fn drop(&mut self) {
        for (key, value) in &self.previous {
            match value {
                Some(value) => unsafe { std::env::set_var(key, value) },
                None => unsafe { std::env::remove_var(key) },
            }
        }
    }
}

#[test]
fn config_path_honors_exa_agent_config_override() {
    let path = temp_config_path("override");
    let _guard = EnvGuard::set("EXA_AGENT_CONFIG", path.to_str().unwrap());
    assert_eq!(config::config_path(), path);
}

#[test]
fn config_path_ignores_empty_overrides() {
    let home =
        std::env::temp_dir().join(format!("exa-agent-config-test-home-{}", std::process::id()));
    let _guard = MultiEnvGuard::set(&[
        ("EXA_AGENT_CONFIG", Some("")),
        ("XDG_CONFIG_HOME", Some("")),
        ("HOME", Some(home.to_str().unwrap())),
    ]);
    assert_eq!(
        config::config_path(),
        home.join(".config")
            .join("exa-agent-cli")
            .join("config.toml")
    );
}

#[test]
fn missing_config_loads_defaults() {
    let path = temp_config_path("missing");
    let _guard = EnvGuard::set("EXA_AGENT_CONFIG", path.to_str().unwrap());
    let cfg = Config::load().unwrap();
    assert_eq!(cfg.base_url, DEFAULT_BASE_URL);
    assert!(cfg.retry.is_some());
}

#[test]
fn malformed_config_returns_config_parse_error() {
    let path = temp_config_path("malformed");
    fs::write(&path, "base_url = [\n").unwrap();
    let err = Config::load_from_path(&path).unwrap_err();
    match err {
        CliError::Config(d) => assert_eq!(d.code, "config_parse_error"),
        other => panic!("expected Config error, got {other:?}"),
    }
}

#[test]
fn set_get_unset_never_writes_secrets_and_creates_user_config() {
    let path = temp_config_path("roundtrip");
    let _guard = EnvGuard::set("EXA_AGENT_CONFIG", path.to_str().unwrap());

    let secret_err = {
        let mut cfg = Config::default();
        cfg.set_path("api-key", "super-secret")
    };
    assert!(matches!(secret_err, Err(CliError::Config(_))));

    let mut cfg = Config::default();
    cfg.set_path("base-url", "https://api.exa.ai").unwrap();
    cfg.create_profile("work").unwrap();
    cfg.set_path("profiles.work.api-key-env", "EXA_API_KEY_WORK")
        .unwrap();
    cfg.save().unwrap();

    assert!(path.exists());
    let raw = fs::read_to_string(&path).unwrap();
    assert!(!raw.contains("super-secret"));
    assert!(!raw.contains("api_key ="));
    assert!(raw.contains("api_key_env"));

    let loaded = Config::load_from_path(&path).unwrap();
    let value = loaded.get_path("profiles.work.api-key-env").unwrap();
    assert_eq!(
        value,
        Some(serde_json::Value::String("EXA_API_KEY_WORK".to_string()))
    );

    let mut loaded = loaded;
    loaded.unset_path("profiles.work.api-key-env").unwrap();
}

#[test]
fn key_env_fields_accept_only_env_var_names_not_secret_values() {
    let mut cfg = Config::default();
    cfg.create_profile("work").unwrap();

    assert!(cfg
        .set_path("profiles.work.api-key-env", "EXA_API_KEY_WORK")
        .is_ok());
    assert!(matches!(
        cfg.set_path("profiles.work.api-key-env", "sk-exa-secret-1234"),
        Err(CliError::Config(_))
    ));
    assert!(matches!(
        cfg.set_path("profiles.work.service-key-env", "not a var"),
        Err(CliError::Config(_))
    ));
    assert_eq!(
        cfg.get_path("profiles.work.api-key-env").unwrap(),
        Some(serde_json::Value::String("EXA_API_KEY_WORK".to_string()))
    );
}

#[test]
fn base_url_validation_rejects_malformed_https_values() {
    let mut cfg = Config::default();
    for value in ["https://not a url", "https://", "http://api.exa.ai"] {
        assert!(
            matches!(cfg.set_path("base-url", value), Err(CliError::Config(_))),
            "{value}"
        );
    }
    assert!(cfg.set_path("base-url", "https://api.exa.ai").is_ok());
}

#[test]
fn list_and_profiles_json_views() {
    let mut cfg = Config::default();
    cfg.create_profile("work").unwrap();
    cfg.set_path("profiles.work.api-key-env", "EXA_API_KEY_WORK")
        .unwrap();
    let list = cfg.list_json();
    assert_eq!(list["baseUrl"], DEFAULT_BASE_URL);
    let profiles = cfg.profiles_json();
    assert!(profiles["profiles"]["work"]["apiKeyEnv"].is_string());
}
