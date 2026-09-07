//! Auth resolution primitives for the future transport/auth commands.
//!
//! This module is deliberately dispatcher-free: parent code chooses the namespace, supplies
//! one-shot stdin when present, and wires any real OS keyring implementation.

use crate::config::Config;
use crate::error::{CliError, Diag};
use crate::fsutil;
use serde::ser::{Serialize, SerializeStruct, Serializer};
use std::fmt;
use std::path::{Path, PathBuf};

const DEFAULT_PROFILE: &str = "default";
const API_ENV: &str = "EXA_API_KEY";
const SERVICE_ENV: &str = "EXA_SERVICE_KEY";

#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into().trim().to_string();
        (!value.is_empty()).then_some(Self(value))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn last4(&self) -> String {
        if self.0.chars().count() < 4 {
            return "<short>".to_string();
        }
        let mut chars: Vec<char> = self.0.chars().rev().take(4).collect();
        chars.reverse();
        chars.into_iter().collect()
    }

    pub fn fingerprint(&self) -> String {
        let mut hash = 0xcbf29ce484222325_u64;
        for byte in self.0.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        format!("fp_{hash:016x}")
    }

    pub fn redacted(&self) -> String {
        format!("<redacted:{}:{}>", self.last4(), self.fingerprint())
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.redacted())
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Secret").field(&self.redacted()).finish()
    }
}

impl Serialize for Secret {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.redacted())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialNamespace {
    Api,
    Service,
}

impl CredentialNamespace {
    pub fn env_var(self) -> &'static str {
        match self {
            Self::Api => API_ENV,
            Self::Service => SERVICE_ENV,
        }
    }

    pub fn explicit_rung(self) -> &'static str {
        match self {
            Self::Api => "--api-key",
            Self::Service => "--service-key",
        }
    }

    pub fn stdin_rung(self) -> &'static str {
        match self {
            Self::Api => "--api-key-stdin",
            Self::Service => "--service-key-stdin",
        }
    }

    pub fn keyring_service(self, profile: &str) -> String {
        match self {
            Self::Api => format!("exa-agent:api:{profile}"),
            Self::Service => format!("exa-agent:service:{profile}"),
        }
    }

    pub fn credential_file_key(self) -> &'static str {
        match self {
            Self::Api => "api_key",
            Self::Service => "service_key",
        }
    }

    fn suggested_command(self) -> &'static str {
        match self {
            Self::Api => "export EXA_API_KEY=... # or: exa-agent auth login",
            Self::Service => "export EXA_SERVICE_KEY=...",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CredentialSource {
    Explicit,
    Stdin,
    Env(String),
    CredentialFile { path: String },
    Keyring { service: String },
}

impl CredentialSource {
    pub fn label(&self) -> String {
        match self {
            Self::Explicit => "explicit".to_string(),
            Self::Stdin => "stdin".to_string(),
            Self::Env(name) => name.clone(),
            Self::CredentialFile { path } => format!("file:{path}"),
            Self::Keyring { service } => format!("keyring:{service}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedCredential {
    pub namespace: CredentialNamespace,
    pub profile: String,
    pub source: CredentialSource,
    pub secret: Secret,
}

impl ResolvedCredential {
    pub fn status(&self) -> CredentialStatus {
        CredentialStatus {
            namespace: self.namespace,
            profile: self.profile.clone(),
            source: self.source.label(),
            last4: self.secret.last4(),
            fingerprint: self.secret.fingerprint(),
            redacted: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialStatus {
    pub namespace: CredentialNamespace,
    pub profile: String,
    pub source: String,
    pub last4: String,
    pub fingerprint: String,
    pub redacted: bool,
}

impl Serialize for CredentialStatus {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut out = serializer.serialize_struct("CredentialStatus", 6)?;
        out.serialize_field(
            "namespace",
            match self.namespace {
                CredentialNamespace::Api => "api",
                CredentialNamespace::Service => "service",
            },
        )?;
        out.serialize_field("profile", &self.profile)?;
        out.serialize_field("source", &self.source)?;
        out.serialize_field("last4", &self.last4)?;
        out.serialize_field("fingerprint", &self.fingerprint)?;
        out.serialize_field("redacted", &self.redacted)?;
        out.end()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingCredential {
    pub namespace: CredentialNamespace,
    pub profile: String,
    pub checked: Vec<String>,
}

impl MissingCredential {
    pub fn to_error(&self) -> CliError {
        CliError::Auth(
            Diag::new(
                "not_authenticated",
                format!(
                    "no {:?} credential resolved for profile `{}`",
                    self.namespace, self.profile
                ),
            )
            .with_details(serde_json::json!({ "checked": self.checked.clone() }))
            .with_suggestion(self.namespace.suggested_command()),
        )
    }
}

pub trait Keyring {
    fn get(&self, service: &str) -> Result<Option<String>, KeyringError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyringError;

pub struct NoopKeyring;

impl Keyring for NoopKeyring {
    fn get(&self, _service: &str) -> Result<Option<String>, KeyringError> {
        Ok(None)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CredentialInput {
    pub profile: Option<String>,
    pub env_profile: Option<String>,
    pub explicit: Option<String>,
    pub stdin: Option<String>,
    pub env: Option<String>,
    /// The env var name `env` was read from. `None` means the namespace default
    /// (`EXA_API_KEY`/`EXA_SERVICE_KEY`); `Some` carries a profile's `api_key_env`.
    pub env_var_name: Option<String>,
    pub credential_file: Option<String>,
    pub credential_file_path: Option<String>,
}

impl CredentialInput {
    pub fn from_env(
        profile: Option<String>,
        explicit: Option<String>,
        stdin: Option<String>,
        ns: CredentialNamespace,
    ) -> Self {
        let env_profile = std::env::var("EXA_PROFILE").ok();
        let config = Config::load().ok();
        // Select the profile the same way base-url resolution does: explicit --profile / EXA_PROFILE,
        // else the config's active_profile, else "default". Config load failures fall back to the
        // explicit selection (a broken config surfaces via `doctor`/`config`, not here).
        let selected = clean(profile.as_deref()).or_else(|| clean(env_profile.as_deref()));
        let profile_name = match &config {
            Some(cfg) => cfg.effective_profile_name(selected),
            None => selected.unwrap_or(DEFAULT_PROFILE).to_string(),
        };
        let env_var_name = config
            .as_ref()
            .and_then(|cfg| match ns {
                CredentialNamespace::Api => cfg.api_key_env_for_profile(&profile_name),
                CredentialNamespace::Service => cfg.service_key_env_for_profile(&profile_name),
            })
            .filter(|name| !name.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| ns.env_var().to_string());
        Self {
            profile: Some(profile_name),
            env_profile,
            explicit,
            stdin,
            env: std::env::var(&env_var_name).ok(),
            env_var_name: Some(env_var_name),
            credential_file: credential_file_value(ns).ok().flatten(),
            credential_file_path: Some(credentials_path().display().to_string()),
        }
    }
}

/// Resolve the credentials file path used by `auth login` and live smoke. This is secret data,
/// unlike `config.toml`, so callers must enforce 0600 on write.
pub fn credentials_path() -> PathBuf {
    // Same resolution ladder as `config_path()` (explicit override, then absolute XDG, then
    // `$HOME`), so config and credentials can never disagree about which directory they live in.
    crate::config::managed_path(
        "EXA_AGENT_CREDENTIALS",
        "XDG_CONFIG_HOME",
        &[".config"],
        &["credentials.json"],
    )
}

pub fn credential_file_value(ns: CredentialNamespace) -> Result<Option<String>, CliError> {
    let path = credentials_path();
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| {
        CliError::Config(Diag::new(
            "config_invalid",
            format!("failed to read credentials file at {}: {e}", path.display()),
        ))
    })?;
    let value: serde_json::Value = serde_json::from_str(&raw).map_err(|e| {
        CliError::Config(Diag::new(
            "config_invalid",
            format!(
                "failed to parse credentials file at {}: {e}",
                path.display()
            ),
        ))
    })?;
    Ok(value
        .get(ns.credential_file_key())
        .and_then(|v| v.as_str())
        .map(ToOwned::to_owned))
}

/// Run one credential mutation as an atomic read-modify-write under the credentials file's
/// exclusive lock.
///
/// `auth login` for the API namespace and `auth login` for the service namespace share one
/// JSON file. Without the lock, two concurrent logins each read the pre-existing object, add
/// their own key, and write a full replacement — the second rename silently discards the first
/// agent's freshly stored key. A unique temp name does not help: both writes are individually
/// well-formed.
fn update_credentials<T>(
    f: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>) -> T,
) -> Result<(PathBuf, T), CliError> {
    update_credentials_at(credentials_path(), f)
}

fn update_credentials_at<T>(
    path: PathBuf,
    f: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>) -> T,
) -> Result<(PathBuf, T), CliError> {
    fsutil::create_parent_dir_private(&path).map_err(|e| {
        CliError::Config(Diag::new(
            "config_invalid",
            format!(
                "failed to create credentials directory for {}: {e}",
                path.display()
            ),
        ))
    })?;
    let value = fsutil::with_lock(
        &path,
        |e| {
            CliError::Config(Diag::new(
                "config_invalid",
                format!("failed to lock credentials file {}: {e}", path.display()),
            ))
        },
        || {
            let mut object = match read_credentials_json_at(&path)? {
                Some(serde_json::Value::Object(map)) => map,
                _ => serde_json::Map::new(),
            };
            let value = f(&mut object);
            if object.is_empty() {
                remove_credentials_file(&path)?;
            } else {
                write_credentials_json(&path, &serde_json::Value::Object(object))?;
            }
            Ok(value)
        },
    )?;
    Ok((path, value))
}

pub fn write_credential_file(
    ns: CredentialNamespace,
    secret: &Secret,
) -> Result<PathBuf, CliError> {
    let (path, ()) = update_credentials(|object| {
        object.insert(
            ns.credential_file_key().to_string(),
            serde_json::Value::String(secret.expose().to_string()),
        );
    })?;
    Ok(path)
}

pub fn clear_credential_file(ns: CredentialNamespace) -> Result<PathBuf, CliError> {
    let (path, ()) = update_credentials(|object| {
        object.remove(ns.credential_file_key());
    })?;
    Ok(path)
}

fn remove_credentials_file(path: &Path) -> Result<(), CliError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(CliError::Config(Diag::new(
            "config_invalid",
            format!(
                "failed to remove credentials file {}: {err}",
                path.display()
            ),
        ))),
    }
}

fn read_credentials_json_at(path: &Path) -> Result<Option<serde_json::Value>, CliError> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(path).map_err(|e| {
        CliError::Config(Diag::new(
            "config_invalid",
            format!("failed to read credentials file at {}: {e}", path.display()),
        ))
    })?;
    let value: serde_json::Value = serde_json::from_str(&raw).map_err(|e| {
        CliError::Config(Diag::new(
            "config_invalid",
            format!(
                "failed to parse credentials file at {}: {e}",
                path.display()
            ),
        ))
    })?;
    Ok(Some(value))
}

fn write_credentials_json(path: &Path, value: &serde_json::Value) -> Result<(), CliError> {
    // Create a *new* managed directory 0700; never chmod one that already exists. The parent of
    // an explicit `EXA_AGENT_CREDENTIALS` path, or a `~/.config` shared with every other tool on
    // the machine, belongs to its owner — `doctor` reports a permissive one instead.
    fsutil::create_parent_dir_private(path).map_err(|e| {
        CliError::Config(Diag::new(
            "config_invalid",
            format!(
                "failed to create credentials directory for {}: {e}",
                path.display()
            ),
        ))
    })?;
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|e| {
        CliError::Config(Diag::new(
            "config_invalid",
            format!("failed to serialize credentials file: {e}"),
        ))
    })?;
    bytes.push(b'\n');
    // 0600 from creation (not a post-write chmod), unique temp, fsync, atomic rename.
    fsutil::write_private_atomic(path, &bytes).map_err(|e| {
        CliError::Config(Diag::new(
            "config_invalid",
            format!("failed to write credentials file {}: {e}", path.display()),
        ))
    })
}

/// Cheap shape check for API keys, used to avoid accepting an API key in service-key flows.
pub fn looks_like_api_key(key: &str) -> bool {
    let k = key.trim().to_ascii_lowercase();
    k.starts_with("exa-") || k.starts_with("sk-exa") || k.starts_with("sk_exa") || is_uuid_like(&k)
}

/// Cheap shape check for service/admin keys, used to avoid sending obvious service keys to API flows.
pub fn looks_like_service_key(key: &str) -> bool {
    let k = key.trim().to_ascii_lowercase();
    k.starts_with("svc-")
        || k.starts_with("svc_")
        || k.starts_with("service-")
        || k.starts_with("service_")
}

fn is_uuid_like(token: &str) -> bool {
    let parts: Vec<&str> = token.split('-').collect();
    let lens = [8, 4, 4, 4, 12];
    parts.len() == lens.len()
        && parts
            .iter()
            .zip(lens)
            .all(|(part, len)| part.len() == len && part.chars().all(|c| c.is_ascii_hexdigit()))
}

pub fn resolve_profile(profile: Option<&str>, env_profile: Option<&str>) -> String {
    clean(profile)
        .or_else(|| clean(env_profile))
        .unwrap_or(DEFAULT_PROFILE)
        .to_string()
}

pub fn resolve_credential<K: Keyring>(
    namespace: CredentialNamespace,
    input: &CredentialInput,
    keyring: &K,
) -> Result<ResolvedCredential, MissingCredential> {
    let profile = resolve_profile(input.profile.as_deref(), input.env_profile.as_deref());
    let mut checked = Vec::new();

    checked.push(namespace.explicit_rung().to_string());
    if let Some(secret) = input.explicit.as_deref().and_then(Secret::new) {
        return Ok(found(
            namespace,
            profile,
            CredentialSource::Explicit,
            secret,
        ));
    }

    checked.push(namespace.stdin_rung().to_string());
    if let Some(secret) = input.stdin.as_deref().and_then(Secret::new) {
        return Ok(found(namespace, profile, CredentialSource::Stdin, secret));
    }

    let env_var_name = input
        .env_var_name
        .clone()
        .unwrap_or_else(|| namespace.env_var().to_string());
    checked.push(env_var_name.clone());
    if let Some(secret) = input.env.as_deref().and_then(Secret::new) {
        return Ok(found(
            namespace,
            profile,
            CredentialSource::Env(env_var_name),
            secret,
        ));
    }

    let file_path = input
        .credential_file_path
        .clone()
        .unwrap_or_else(|| credentials_path().display().to_string());
    checked.push(format!(
        "file:{file_path}:{}",
        namespace.credential_file_key()
    ));
    if let Some(secret) = input.credential_file.as_deref().and_then(Secret::new) {
        return Ok(found(
            namespace,
            profile,
            CredentialSource::CredentialFile { path: file_path },
            secret,
        ));
    }

    let service = namespace.keyring_service(&profile);
    checked.push(format!("keyring:{service}"));
    if let Ok(Some(raw)) = keyring.get(&service) {
        if let Some(secret) = Secret::new(raw) {
            return Ok(found(
                namespace,
                profile,
                CredentialSource::Keyring { service },
                secret,
            ));
        }
    }

    Err(MissingCredential {
        namespace,
        profile,
        checked,
    })
}

pub fn resolve_api_credential<K: Keyring>(
    input: &CredentialInput,
    keyring: &K,
) -> Result<ResolvedCredential, MissingCredential> {
    resolve_credential(CredentialNamespace::Api, input, keyring)
}

pub fn resolve_service_credential<K: Keyring>(
    input: &CredentialInput,
    keyring: &K,
) -> Result<ResolvedCredential, MissingCredential> {
    resolve_credential(CredentialNamespace::Service, input, keyring)
}

pub fn not_authenticated_error(missing: &MissingCredential) -> CliError {
    missing.to_error()
}

fn found(
    namespace: CredentialNamespace,
    profile: String,
    source: CredentialSource,
    secret: Secret,
) -> ResolvedCredential {
    ResolvedCredential {
        namespace,
        profile,
        source,
        secret,
    }
}

fn clean(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod mutation_tests {
    use super::*;

    #[test]
    fn concurrent_credential_namespaces_survive_without_global_environment() {
        let path = std::env::temp_dir().join(format!(
            "exa-credential-race-{}-{}.json",
            std::process::id(),
            crate::transport::new_request_id()
        ));
        std::thread::scope(|scope| {
            for _ in 0..4 {
                for name in ["api_key", "service_key"] {
                    let path = &path;
                    scope.spawn(move || {
                        update_credentials_at(path.clone(), |object| {
                            object.insert(name.to_string(), serde_json::json!("fixture-only"));
                        })
                        .unwrap();
                    });
                }
            }
        });
        let value = read_credentials_json_at(&path).unwrap().unwrap();
        assert_eq!(value.as_object().unwrap().len(), 2);
        assert_eq!(value["api_key"], "fixture-only");
        assert_eq!(value["service_key"], "fixture-only");
        update_credentials_at(path.clone(), |object| {
            object.remove("api_key");
        })
        .unwrap();
        let value = read_credentials_json_at(&path).unwrap().unwrap();
        assert!(value.get("api_key").is_none());
        assert_eq!(value["service_key"], "fixture-only");
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_file(crate::fsutil::lock_path_for(&path)).unwrap();
    }
}
