//! Auth resolution primitives for the future transport/auth commands.
//!
//! This module is deliberately dispatcher-free: parent code chooses the namespace, supplies
//! one-shot stdin when present, and wires any real OS keyring implementation.

use crate::error::{CliError, Diag};
use serde::ser::{Serialize, SerializeStruct, Serializer};
use std::fmt;
use std::io::Write;
use std::path::PathBuf;

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
    Env(&'static str),
    CredentialFile { path: String },
    Keyring { service: String },
}

impl CredentialSource {
    pub fn label(&self) -> String {
        match self {
            Self::Explicit => "explicit".to_string(),
            Self::Stdin => "stdin".to_string(),
            Self::Env(name) => (*name).to_string(),
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
        Self {
            profile,
            env_profile: std::env::var("EXA_PROFILE").ok(),
            explicit,
            stdin,
            env: std::env::var(ns.env_var()).ok(),
            credential_file: credential_file_value(ns).ok().flatten(),
            credential_file_path: Some(credentials_path().display().to_string()),
        }
    }
}

/// Resolve the credentials file path used by `auth login` and live smoke. This is secret data,
/// unlike `config.toml`, so callers must enforce 0600 on write.
pub fn credentials_path() -> PathBuf {
    if let Ok(path) = std::env::var("EXA_AGENT_CREDENTIALS") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.trim().is_empty() {
            return PathBuf::from(xdg)
                .join("exa-agent-cli")
                .join("credentials.json");
        }
    }
    std::env::var("HOME")
        .map(|home| {
            PathBuf::from(home)
                .join(".config")
                .join("exa-agent-cli")
                .join("credentials.json")
        })
        .unwrap_or_else(|_| PathBuf::from(".config/exa-agent-cli/credentials.json"))
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

pub fn write_credential_file(
    ns: CredentialNamespace,
    secret: &Secret,
) -> Result<PathBuf, CliError> {
    let path = credentials_path();
    let mut value = read_credentials_json()?
        .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));
    if !value.is_object() {
        value = serde_json::Value::Object(serde_json::Map::new());
    }
    let obj = value
        .as_object_mut()
        .expect("credential file root is object");
    obj.insert(
        ns.credential_file_key().to_string(),
        serde_json::Value::String(secret.expose().to_string()),
    );
    write_credentials_json(&path, &value)?;
    Ok(path)
}

pub fn clear_credential_file(ns: CredentialNamespace) -> Result<PathBuf, CliError> {
    let path = credentials_path();
    let Some(mut value) = read_credentials_json()? else {
        return Ok(path);
    };
    if let Some(obj) = value.as_object_mut() {
        obj.remove(ns.credential_file_key());
        if obj.is_empty() {
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => {
                    return Err(CliError::Config(Diag::new(
                        "config_invalid",
                        format!(
                            "failed to remove credentials file {}: {err}",
                            path.display()
                        ),
                    )));
                }
            }
            return Ok(path);
        }
    }
    write_credentials_json(&path, &value)?;
    Ok(path)
}

fn read_credentials_json() -> Result<Option<serde_json::Value>, CliError> {
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
    Ok(Some(value))
}

fn write_credentials_json(path: &PathBuf, value: &serde_json::Value) -> Result<(), CliError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            CliError::Config(Diag::new(
                "config_invalid",
                format!(
                    "failed to create credentials directory {}: {e}",
                    parent.display()
                ),
            ))
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if std::env::var_os("EXA_AGENT_CREDENTIALS").is_none() {
                std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700)).map_err(
                    |e| {
                        CliError::Config(Diag::new(
                            "config_invalid",
                            format!(
                                "failed to secure credentials directory {}: {e}",
                                parent.display()
                            ),
                        ))
                    },
                )?;
            }
        }
    }
    let tmp = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| {
        CliError::Config(Diag::new(
            "config_invalid",
            format!("failed to serialize credentials file: {e}"),
        ))
    })?;
    {
        #[cfg(unix)]
        use std::os::unix::fs::OpenOptionsExt;
        let mut options = std::fs::OpenOptions::new();
        options.create(true).truncate(true).write(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&tmp).map_err(|e| {
            CliError::Config(Diag::new(
                "config_invalid",
                format!("failed to open credentials file {}: {e}", tmp.display()),
            ))
        })?;
        file.write_all(&bytes).map_err(|e| {
            CliError::Config(Diag::new(
                "config_invalid",
                format!("failed to write credentials file {}: {e}", tmp.display()),
            ))
        })?;
        file.write_all(b"\n").map_err(|e| {
            CliError::Config(Diag::new(
                "config_invalid",
                format!("failed to write credentials file {}: {e}", tmp.display()),
            ))
        })?;
        file.sync_all().map_err(|e| {
            CliError::Config(Diag::new(
                "config_invalid",
                format!("failed to sync credentials file {}: {e}", tmp.display()),
            ))
        })?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600)).map_err(|e| {
            CliError::Config(Diag::new(
                "config_invalid",
                format!("failed to secure credentials file {}: {e}", tmp.display()),
            ))
        })?;
    }
    std::fs::rename(&tmp, path).map_err(|e| {
        CliError::Config(Diag::new(
            "config_invalid",
            format!("failed to install credentials file {}: {e}", path.display()),
        ))
    })?;
    Ok(())
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

    checked.push(namespace.env_var().to_string());
    if let Some(secret) = input.env.as_deref().and_then(Secret::new) {
        return Ok(found(
            namespace,
            profile,
            CredentialSource::Env(namespace.env_var()),
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
