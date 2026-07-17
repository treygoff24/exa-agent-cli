//! Diagnostics and reversible config repair (D8 upgrade path). Offline by default.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::auth::{self, CredentialNamespace};
use crate::config::{self, Config};
use crate::error::CliError;
use crate::pending;
use crate::registry::{BUILD_DATE, EMBEDDED_SPEC_SHA256, GIT_SHA, SPEC_VERSION, TARGET};
use crate::transport::AuthProbe;

/// Results of the networked probes, computed by dispatch when `--online` is set and injected
/// into [`DoctorCtx`] so the detectors stay pure. `None` on a field means "not probed".
#[derive(Debug)]
pub struct OnlineProbes {
    /// `Ok(status)` if the base host answered; `Err(message)` on a transport-level failure.
    pub connectivity: Result<u16, String>,
    /// `None` when no credential resolved (nothing to test); else the auth-probe outcome.
    pub auth: Option<Result<AuthProbe, String>>,
}

pub const DOCTOR_SCHEMA: &str = "exa.cli.doctor.v1";

pub const DETECTOR_IDS: &[&str] = &[
    "config.parse",
    "config.format",
    "permissions.config",
    "permissions.credentials",
    "state.stale-cache",
    "key.present",
    "service-key.scope",
    "base-url",
    "spec.hash",
    "binary.version",
    "tty.discipline",
    "auth.online",
    "connectivity",
];

/// Options mirroring `cli::DoctorArgs` so dispatch can wire without coupling.
#[derive(Debug, Clone, Default)]
pub struct DoctorOptions {
    pub online: bool,
    pub checks: Vec<String>,
    pub fix: bool,
    pub dry_run: bool,
    pub undo: bool,
    pub allow_auth: bool,
    pub allow_delete: bool,
}

/// Injectable environment for tests and dispatch.
#[derive(Debug)]
pub struct DoctorCtx {
    pub config_path: PathBuf,
    pub config_load: Result<Config, CliError>,
    pub credentials_path: PathBuf,
    pub state_dir: PathBuf,
    pub api_key: Option<String>,
    pub service_key: Option<String>,
    pub stdout_is_tty: bool,
    /// When set (tests), `spec.hash` compares against this instead of always passing.
    pub expected_spec_sha256: Option<String>,
    /// Populated by dispatch when `--online`; `None` offline. Detectors read, never probe.
    pub online_probes: Option<OnlineProbes>,
}

impl DoctorCtx {
    pub fn from_process() -> Self {
        Self {
            config_path: config::config_path(),
            config_load: Config::load(),
            credentials_path: auth::credentials_path(),
            state_dir: pending::state_dir(),
            api_key: std::env::var("EXA_API_KEY").ok().or_else(|| {
                auth::credential_file_value(CredentialNamespace::Api)
                    .ok()
                    .flatten()
            }),
            service_key: std::env::var("EXA_SERVICE_KEY").ok().or_else(|| {
                auth::credential_file_value(CredentialNamespace::Service)
                    .ok()
                    .flatten()
            }),
            stdout_is_tty: crate::output::stdout_is_tty(),
            expected_spec_sha256: None,
            online_probes: None,
        }
    }

    pub fn with_config_path(mut self, path: PathBuf) -> Self {
        self.config_path = path.clone();
        self.config_load = Config::load_from_path(&path);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FindingStatus {
    Ok,
    Warn,
    Fail,
    Skip,
    Refused,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    pub id: &'static str,
    pub status: FindingStatus,
    pub category: &'static str,
    #[serde(rename = "suggestedCommand", skip_serializing_if = "Option::is_none")]
    pub suggested_command: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DoctorStatus {
    Healthy,
    Findings,
    Refused,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DoctorReport {
    pub schema: &'static str,
    pub ok: bool,
    pub status: DoctorStatus,
    pub findings: Vec<Finding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<FixAction>,
    #[serde(rename = "backupPath", skip_serializing_if = "Option::is_none")]
    pub backup_path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FixStatus {
    Planned,
    Fixed,
    Skipped,
    Restored,
    Refused,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FixAction {
    pub id: &'static str,
    pub status: FixStatus,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(rename = "requiredFlag", skip_serializing_if = "Option::is_none")]
    pub required_flag: Option<&'static str>,
}

impl DoctorReport {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

pub fn run_doctor(options: &DoctorOptions, ctx: &DoctorCtx) -> DoctorReport {
    if options.undo {
        return undo_latest(ctx);
    }
    let mut findings = Vec::new();
    for id in DETECTOR_IDS {
        if !options.checks.is_empty() && !options.checks.iter().any(|c| c == id) {
            continue;
        }
        let finding = match *id {
            "config.parse" => detect_config_parse(ctx),
            "config.format" => detect_config_format(ctx),
            "permissions.config" => {
                detect_permissions("permissions.config", &ctx.config_path, false)
            }
            "permissions.credentials" => {
                detect_permissions("permissions.credentials", &ctx.credentials_path, true)
            }
            "state.stale-cache" => detect_stale_cache(ctx),
            "key.present" => detect_key_present(ctx),
            "service-key.scope" => detect_service_key_scope(ctx),
            "base-url" => detect_base_url(ctx),
            "spec.hash" => detect_spec_hash(ctx),
            "binary.version" => detect_binary_version(),
            "tty.discipline" => detect_tty_discipline(ctx),
            "auth.online" => detect_auth_online(options, ctx),
            "connectivity" => detect_connectivity(options, ctx),
            _ => continue,
        };
        findings.push(finding);
    }

    let mut actions = Vec::new();
    let mut backup_path = None;
    if options.fix {
        apply_fixes(options, ctx, &mut findings, &mut actions, &mut backup_path);
    }
    let status = summarize_status(&findings);
    let ok = matches!(status, DoctorStatus::Healthy);
    DoctorReport {
        schema: DOCTOR_SCHEMA,
        ok,
        status,
        findings,
        actions,
        backup_path,
    }
}

pub fn validate_check_ids(checks: &[String]) -> Result<(), CliError> {
    let unknown: Vec<&str> = checks
        .iter()
        .map(String::as_str)
        .filter(|check| !DETECTOR_IDS.contains(check))
        .collect();
    if unknown.is_empty() {
        return Ok(());
    }
    Err(CliError::Usage(
        crate::error::Diag::new(
            "invalid_value",
            format!("unknown doctor check id `{}`", unknown[0]),
        )
        .with_details(serde_json::json!({
            "unknown": unknown,
            "valid": DETECTOR_IDS,
        }))
        .with_suggestion("exa-agent doctor --check key.present"),
    ))
}

pub fn doctor_exit_code(report: &DoctorReport) -> i32 {
    match report.status {
        DoctorStatus::Healthy => 0,
        DoctorStatus::Findings => 1,
        DoctorStatus::Refused => 4,
    }
}

fn summarize_status(findings: &[Finding]) -> DoctorStatus {
    if findings.iter().any(|f| f.status == FindingStatus::Refused) {
        return DoctorStatus::Refused;
    }
    if findings
        .iter()
        .any(|f| matches!(f.status, FindingStatus::Fail | FindingStatus::Warn))
    {
        return DoctorStatus::Findings;
    }
    DoctorStatus::Healthy
}

fn detect_config_parse(ctx: &DoctorCtx) -> Finding {
    match &ctx.config_load {
        Ok(cfg) => {
            if let Some(name) = cfg.active_profile.as_deref() {
                if !cfg.profiles.contains_key(name) {
                    return Finding {
                        id: "config.parse",
                        status: FindingStatus::Fail,
                        category: "config",
                        message: format!("active profile `{name}` is not defined in config"),
                        suggested_command: Some("exa-agent config profiles list".to_string()),
                    };
                }
            }
            Finding {
                id: "config.parse",
                status: FindingStatus::Ok,
                category: "config",
                message: format!("config at {} parses", ctx.config_path.display()),
                suggested_command: None,
            }
        }
        Err(err) => Finding {
            id: "config.parse",
            status: FindingStatus::Fail,
            category: "config",
            message: err.diag().message.clone(),
            suggested_command: Some("exa-agent config path".to_string()),
        },
    }
}

fn detect_config_format(ctx: &DoctorCtx) -> Finding {
    if !ctx.config_path.exists() {
        return ok_finding(
            "config.format",
            "config",
            format!(
                "config at {} does not exist; defaults apply",
                ctx.config_path.display()
            ),
        );
    }
    let formatted = formatted_config(&ctx.config_path);
    match formatted {
        Ok(formatted) => match fs::read_to_string(&ctx.config_path) {
            Ok(raw) if raw == formatted => ok_finding(
                "config.format",
                "config",
                format!(
                    "config at {} is canonically formatted",
                    ctx.config_path.display()
                ),
            ),
            Ok(_) => Finding {
                id: "config.format",
                status: FindingStatus::Warn,
                category: "config",
                suggested_command: Some("exa-agent doctor --fix".to_string()),
                message: format!(
                    "config at {} needs canonical TOML formatting",
                    ctx.config_path.display()
                ),
            },
            Err(error) => refused_finding(
                "config.format",
                "config",
                format!(
                    "cannot read config at {}: {error}",
                    ctx.config_path.display()
                ),
            ),
        },
        Err(message) if ctx.config_load.is_err() => Finding {
            id: "config.format",
            status: FindingStatus::Skip,
            category: "config",
            suggested_command: Some("exa-agent config path".to_string()),
            message,
        },
        Err(message) => refused_finding("config.format", "config", message),
    }
}

fn detect_permissions(id: &'static str, path: &Path, auth_file: bool) -> Finding {
    if !path.exists() {
        return ok_finding(
            id,
            "permissions",
            format!("{} does not exist", path.display()),
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        match fs::metadata(path) {
            Ok(metadata) => {
                let mode = metadata.permissions().mode() & 0o777;
                if mode == 0o600 {
                    return ok_finding(
                        id,
                        "permissions",
                        format!("{} has mode 0600", path.display()),
                    );
                }
                let command = if auth_file {
                    "exa-agent doctor --fix --allow-auth"
                } else {
                    "exa-agent doctor --fix"
                };
                Finding {
                    id,
                    status: FindingStatus::Warn,
                    category: "permissions",
                    suggested_command: Some(command.to_string()),
                    message: format!("{} has mode {mode:04o}; expected 0600", path.display()),
                }
            }
            Err(error) => refused_finding(
                id,
                "permissions",
                format!("cannot inspect permissions for {}: {error}", path.display()),
            ),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = auth_file;
        Finding {
            id,
            status: FindingStatus::Skip,
            category: "permissions",
            suggested_command: None,
            message: "POSIX permission-bit checks do not apply on this platform".to_string(),
        }
    }
}

fn detect_stale_cache(ctx: &DoctorCtx) -> Finding {
    match stale_spill_files(ctx) {
        Ok(files) if files.is_empty() => ok_finding(
            "state.stale-cache",
            "state",
            format!(
                "no stale spill files under {}",
                ctx.state_dir.join("spill").display()
            ),
        ),
        Ok(files) => Finding {
            id: "state.stale-cache",
            status: FindingStatus::Warn,
            category: "state",
            suggested_command: Some("exa-agent doctor --fix --allow-delete".to_string()),
            message: format!(
                "{} spill file(s) are older than 7 days under {}",
                files.len(),
                ctx.state_dir.join("spill").display()
            ),
        },
        Err(message) => refused_finding("state.stale-cache", "state", message),
    }
}

fn ok_finding(id: &'static str, category: &'static str, message: String) -> Finding {
    Finding {
        id,
        status: FindingStatus::Ok,
        category,
        suggested_command: None,
        message,
    }
}

fn refused_finding(id: &'static str, category: &'static str, message: String) -> Finding {
    Finding {
        id,
        status: FindingStatus::Refused,
        category,
        suggested_command: Some("exa-agent doctor".to_string()),
        message,
    }
}

fn formatted_config(path: &Path) -> Result<String, String> {
    let raw = fs::read_to_string(path)
        .map_err(|error| format!("cannot read config at {}: {error}", path.display()))?;
    let mut document: toml_edit::DocumentMut = raw.parse().map_err(|error| {
        format!(
            "cannot format malformed config at {}: {error}",
            path.display()
        )
    })?;
    format_table(document.as_table_mut());
    Ok(document.to_string())
}

fn format_table(table: &mut toml_edit::Table) {
    for (mut key, item) in table.iter_mut() {
        if let Some(value) = item.as_value_mut() {
            // Keep comment-bearing prefixes/suffixes; normalize only spacing around `=`.
            key.leaf_decor_mut().set_suffix(" ");
            value.decor_mut().set_prefix(" ");
        } else if let Some(child) = item.as_table_mut() {
            format_table(child);
        } else if let Some(array) = item.as_array_of_tables_mut() {
            for child in array.iter_mut() {
                format_table(child);
            }
        }
    }
}

const STALE_SPILL_SECONDS: u64 = 7 * 24 * 60 * 60;

fn stale_spill_files(ctx: &DoctorCtx) -> Result<Vec<PathBuf>, String> {
    let dir = ctx.state_dir.join("spill");
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let now = now_epoch_seconds();
    let entries = fs::read_dir(&dir)
        .map_err(|error| format!("cannot inspect spill directory {}: {error}", dir.display()))?;
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("cannot inspect spill entry: {error}"))?;
        let metadata = entry
            .metadata()
            .map_err(|error| format!("cannot inspect {}: {error}", entry.path().display()))?;
        if !metadata.is_file() {
            continue;
        }
        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs())
            .unwrap_or(now);
        if now.saturating_sub(modified) >= STALE_SPILL_SECONDS {
            files.push(entry.path());
        }
    }
    files.sort();
    Ok(files)
}

fn detect_key_present(ctx: &DoctorCtx) -> Finding {
    if ctx.api_key.is_some() {
        return Finding {
            id: "key.present",
            status: FindingStatus::Ok,
            category: "auth",
            message: "API key resolved locally".to_string(),
            suggested_command: None,
        };
    }
    if let Ok(cfg) = &ctx.config_load {
        if let Some(profile) = cfg.active_profile() {
            if let Some(env_name) = profile.api_key_env.as_deref() {
                if std::env::var(env_name).is_ok() {
                    return Finding {
                        id: "key.present",
                        status: FindingStatus::Ok,
                        category: "auth",
                        message: format!("{env_name} resolved for active profile"),
                        suggested_command: None,
                    };
                }
            }
        }
    }
    Finding {
        id: "key.present",
        status: FindingStatus::Warn,
        category: "auth",
        message: "no API key found in EXA_API_KEY or profile env".to_string(),
        suggested_command: Some("export EXA_API_KEY=…".to_string()),
    }
}

fn detect_service_key_scope(ctx: &DoctorCtx) -> Finding {
    let Some(key) = ctx.service_key.as_deref() else {
        return Finding {
            id: "service-key.scope",
            status: FindingStatus::Ok,
            category: "auth",
            message: "EXA_SERVICE_KEY not set (admin commands need it)".to_string(),
            suggested_command: None,
        };
    };
    if auth::looks_like_api_key(key) {
        return Finding {
            id: "service-key.scope",
            status: FindingStatus::Fail,
            category: "auth",
            message: "EXA_SERVICE_KEY looks like an API key, not a service key".to_string(),
            suggested_command: Some(
                "export EXA_SERVICE_KEY=…  # must be a service key, not EXA_API_KEY".to_string(),
            ),
        };
    }
    Finding {
        id: "service-key.scope",
        status: FindingStatus::Ok,
        category: "auth",
        message: "EXA_SERVICE_KEY shape looks valid".to_string(),
        suggested_command: None,
    }
}

fn detect_base_url(ctx: &DoctorCtx) -> Finding {
    let url = ctx
        .config_load
        .as_ref()
        .map(|cfg| cfg.effective_base_url().to_string())
        .unwrap_or_else(|_| config::DEFAULT_BASE_URL.to_string());
    if config::is_valid_https_url(&url) {
        Finding {
            id: "base-url",
            status: FindingStatus::Ok,
            category: "config",
            message: format!("base URL `{url}` is valid"),
            suggested_command: None,
        }
    } else {
        Finding {
            id: "base-url",
            status: FindingStatus::Fail,
            category: "config",
            message: format!("base URL `{url}` is not a well-formed absolute https URL"),
            suggested_command: Some(format!("exa-agent config set base-url {url}")),
        }
    }
}

fn detect_spec_hash(ctx: &DoctorCtx) -> Finding {
    if let Some(expected) = &ctx.expected_spec_sha256 {
        if expected == EMBEDDED_SPEC_SHA256 {
            return Finding {
                id: "spec.hash",
                status: FindingStatus::Ok,
                category: "config",
                message: "embedded spec SHA matches expected snapshot".to_string(),
                suggested_command: None,
            };
        }
        return Finding {
            id: "spec.hash",
            status: FindingStatus::Warn,
            category: "config",
            message: "embedded spec differs from committed snapshot".to_string(),
            suggested_command: Some("exa-agent schema refresh --check".to_string()),
        };
    }
    Finding {
        id: "spec.hash",
        status: FindingStatus::Ok,
        category: "config",
        message: format!("embedded spec SHA {EMBEDDED_SPEC_SHA256}"),
        suggested_command: None,
    }
}

fn detect_binary_version() -> Finding {
    Finding {
        id: "binary.version",
        status: FindingStatus::Ok,
        category: "binary",
        message: format!(
            "exa-agent {} (spec {}, git {}, built {}, target {})",
            env!("CARGO_PKG_VERSION"),
            SPEC_VERSION,
            GIT_SHA,
            BUILD_DATE,
            TARGET
        ),
        suggested_command: None,
    }
}

fn detect_tty_discipline(ctx: &DoctorCtx) -> Finding {
    if ctx.stdout_is_tty {
        Finding {
            id: "tty.discipline",
            status: FindingStatus::Warn,
            category: "output",
            message: "stdout is a TTY; use --format json or pipe for agent-safe output".to_string(),
            suggested_command: Some("exa-agent capabilities --json".to_string()),
        }
    } else {
        Finding {
            id: "tty.discipline",
            status: FindingStatus::Ok,
            category: "output",
            message: "stdout is not a TTY; JSON/NDJSON discipline OK".to_string(),
            suggested_command: None,
        }
    }
}

fn detect_auth_online(options: &DoctorOptions, ctx: &DoctorCtx) -> Finding {
    if !options.online {
        return skipped_online("auth.online");
    }
    let probe = ctx.online_probes.as_ref().and_then(|p| p.auth.as_ref());
    let (status, message, suggested) = match probe {
        Some(Ok(AuthProbe::Accepted { status })) => (
            FindingStatus::Ok,
            format!("credential accepted upstream (HTTP {status})"),
            None,
        ),
        Some(Ok(AuthProbe::Rejected { status })) => (
            FindingStatus::Fail,
            format!("credential rejected upstream (HTTP {status})"),
            Some("exa-agent auth login".to_string()),
        ),
        Some(Ok(AuthProbe::Inconclusive { status })) => (
            FindingStatus::Warn,
            format!("could not verify credential; upstream returned HTTP {status}"),
            Some("exa-agent doctor --online".to_string()),
        ),
        Some(Err(err)) => (
            FindingStatus::Warn,
            format!("auth probe could not complete: {err}"),
            Some("exa-agent doctor --online".to_string()),
        ),
        None => (
            FindingStatus::Skip,
            "no credential resolved to probe (see key.present)".to_string(),
            Some("exa-agent auth login".to_string()),
        ),
    };
    Finding {
        id: "auth.online",
        status,
        category: "auth",
        message,
        suggested_command: suggested,
    }
}

fn detect_connectivity(options: &DoctorOptions, ctx: &DoctorCtx) -> Finding {
    if !options.online {
        return skipped_online("connectivity");
    }
    let base = ctx
        .config_load
        .as_ref()
        .map(|cfg| cfg.effective_base_url().to_string())
        .unwrap_or_else(|_| config::DEFAULT_BASE_URL.to_string());
    let (status, message, suggested) = match ctx.online_probes.as_ref().map(|p| &p.connectivity) {
        Some(Ok(http_status)) => (
            FindingStatus::Ok,
            format!("`{base}` reachable (HTTP {http_status})"),
            None,
        ),
        Some(Err(err)) => (
            FindingStatus::Fail,
            format!("cannot reach `{base}`: {err}"),
            Some("exa-agent doctor --online".to_string()),
        ),
        None => (
            FindingStatus::Skip,
            format!("connectivity to `{base}` was not probed"),
            Some("exa-agent doctor --online".to_string()),
        ),
    };
    Finding {
        id: "connectivity",
        status,
        category: "network",
        message,
        suggested_command: suggested,
    }
}

fn skipped_online(id: &'static str) -> Finding {
    Finding {
        id,
        status: FindingStatus::Skip,
        category: "network",
        message: "skipped (offline mode; pass --online)".to_string(),
        suggested_command: Some("exa-agent doctor --online".to_string()),
    }
}

fn apply_fixes(
    options: &DoctorOptions,
    ctx: &DoctorCtx,
    findings: &mut Vec<Finding>,
    actions: &mut Vec<FixAction>,
    backup_path: &mut Option<String>,
) {
    let needs_backup = findings.iter().any(|finding| {
        matches!(finding.status, FindingStatus::Warn | FindingStatus::Fail)
            && match finding.id {
                "config.format" | "permissions.config" => true,
                "permissions.credentials" => options.allow_auth,
                "state.stale-cache" => options.allow_delete,
                _ => false,
            }
    });

    if needs_backup && !options.dry_run && ctx.config_path.exists() {
        match backup_config(&ctx.config_path) {
            Ok(path) => *backup_path = Some(path.display().to_string()),
            Err(reason) => {
                findings.push(refused_finding("config.backup", "config", reason.clone()));
                actions.push(FixAction {
                    id: "config.backup",
                    status: FixStatus::Refused,
                    path: ctx.config_path.display().to_string(),
                    reason: Some(reason),
                    required_flag: None,
                });
                return;
            }
        }
    }

    let candidates: Vec<&'static str> = findings
        .iter()
        .filter(|finding| matches!(finding.status, FindingStatus::Warn | FindingStatus::Fail))
        .map(|finding| finding.id)
        .collect();
    for id in candidates {
        match id {
            "config.format" => run_fix(
                id,
                &ctx.config_path,
                options.dry_run,
                findings,
                actions,
                || {
                    let formatted = formatted_config(&ctx.config_path)?;
                    write_atomic(&ctx.config_path, formatted.as_bytes())
                },
            ),
            "permissions.config" => {
                run_permission_fix(id, &ctx.config_path, options.dry_run, findings, actions)
            }
            "permissions.credentials" if !options.allow_auth => actions.push(FixAction {
                id,
                status: FixStatus::Skipped,
                path: ctx.credentials_path.display().to_string(),
                reason: Some(
                    "credential permissions touch authentication data; opt in explicitly"
                        .to_string(),
                ),
                required_flag: Some("--allow-auth"),
            }),
            "permissions.credentials" => run_permission_fix(
                id,
                &ctx.credentials_path,
                options.dry_run,
                findings,
                actions,
            ),
            "state.stale-cache" if !options.allow_delete => actions.push(FixAction {
                id,
                status: FixStatus::Skipped,
                path: ctx.state_dir.join("spill").display().to_string(),
                reason: Some(
                    "removing stale spill files deletes local data; opt in explicitly".to_string(),
                ),
                required_flag: Some("--allow-delete"),
            }),
            "state.stale-cache" => run_fix(
                id,
                &ctx.state_dir.join("spill"),
                options.dry_run,
                findings,
                actions,
                || {
                    for path in stale_spill_files(ctx)? {
                        fs::remove_file(&path).map_err(|error| {
                            format!("failed to delete stale spill {}: {error}", path.display())
                        })?;
                    }
                    Ok(())
                },
            ),
            _ => {}
        }
    }
}

fn run_permission_fix(
    id: &'static str,
    path: &Path,
    dry_run: bool,
    findings: &mut [Finding],
    actions: &mut Vec<FixAction>,
) {
    run_fix(id, path, dry_run, findings, actions, || {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|error| {
                format!("failed to set mode 0600 on {}: {error}", path.display())
            })?;
        }
        Ok(())
    });
}

fn run_fix<F>(
    id: &'static str,
    path: &Path,
    dry_run: bool,
    findings: &mut [Finding],
    actions: &mut Vec<FixAction>,
    fix: F,
) where
    F: FnOnce() -> Result<(), String>,
{
    if dry_run {
        actions.push(FixAction {
            id,
            status: FixStatus::Planned,
            path: path.display().to_string(),
            reason: None,
            required_flag: None,
        });
        return;
    }
    match fix() {
        Ok(()) => {
            if let Some(finding) = findings.iter_mut().find(|finding| finding.id == id) {
                finding.status = FindingStatus::Ok;
                finding.suggested_command = None;
                finding.message = format!("fixed {}", path.display());
            }
            actions.push(FixAction {
                id,
                status: FixStatus::Fixed,
                path: path.display().to_string(),
                reason: None,
                required_flag: None,
            });
        }
        Err(reason) => {
            if let Some(finding) = findings.iter_mut().find(|finding| finding.id == id) {
                finding.status = FindingStatus::Refused;
                finding.message = reason.clone();
            }
            actions.push(FixAction {
                id,
                status: FixStatus::Refused,
                path: path.display().to_string(),
                reason: Some(reason),
                required_flag: None,
            });
        }
    }
}

fn backup_config(config_path: &Path) -> Result<PathBuf, String> {
    let timestamp = now_epoch_seconds();
    let file_name = config_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config.toml");
    let backup = config_path.with_file_name(format!("{file_name}.doctor-backup-{timestamp}"));
    fs::copy(config_path, &backup).map_err(|error| {
        format!(
            "failed to back up config {} to {}: {error}",
            config_path.display(),
            backup.display()
        )
    })?;
    let marker = latest_backup_marker(config_path);
    write_atomic(&marker, backup.display().to_string().as_bytes())?;
    Ok(backup)
}

fn undo_latest(ctx: &DoctorCtx) -> DoctorReport {
    let marker = latest_backup_marker(&ctx.config_path);
    let result = (|| {
        let raw = fs::read_to_string(&marker).map_err(|error| {
            format!(
                "no latest doctor backup for {}: {error}",
                ctx.config_path.display()
            )
        })?;
        let backup = PathBuf::from(raw.trim());
        if !is_backup_for(&backup, &ctx.config_path) {
            return Err(format!(
                "latest doctor marker points outside the config backup scope: {}",
                backup.display()
            ));
        }
        if !backup.is_file() {
            return Err(format!(
                "latest doctor backup {} is missing",
                backup.display()
            ));
        }
        restore_backup(&backup, &ctx.config_path)?;
        fs::remove_file(&marker)
            .map_err(|error| format!("restored config but could not clear undo marker: {error}"))?;
        Ok(backup)
    })();

    match result {
        Ok(backup) => DoctorReport {
            schema: DOCTOR_SCHEMA,
            ok: true,
            status: DoctorStatus::Healthy,
            findings: Vec::new(),
            actions: vec![FixAction {
                id: "config.undo",
                status: FixStatus::Restored,
                path: ctx.config_path.display().to_string(),
                reason: None,
                required_flag: None,
            }],
            backup_path: Some(backup.display().to_string()),
        },
        Err(reason) => DoctorReport {
            schema: DOCTOR_SCHEMA,
            ok: false,
            status: DoctorStatus::Refused,
            findings: vec![refused_finding("config.undo", "config", reason.clone())],
            actions: vec![FixAction {
                id: "config.undo",
                status: FixStatus::Refused,
                path: ctx.config_path.display().to_string(),
                reason: Some(reason),
                required_flag: None,
            }],
            backup_path: None,
        },
    }
}

fn is_backup_for(backup: &Path, config_path: &Path) -> bool {
    let Some(config_name) = config_path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    backup.parent() == config_path.parent()
        && backup
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(&format!("{config_name}.doctor-backup-")))
}

fn restore_backup(backup: &Path, config_path: &Path) -> Result<(), String> {
    let bytes = fs::read(backup)
        .map_err(|error| format!("failed to read backup {}: {error}", backup.display()))?;
    write_atomic(config_path, &bytes)?;
    #[cfg(unix)]
    {
        let permissions = fs::metadata(backup)
            .map_err(|error| format!("failed to inspect backup permissions: {error}"))?
            .permissions();
        fs::set_permissions(config_path, permissions)
            .map_err(|error| format!("failed to restore config permissions: {error}"))?;
    }
    Ok(())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    let tmp = path.with_file_name(format!(
        ".{}.doctor-tmp-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("file"),
        std::process::id()
    ));
    let mut file = fs::File::create(&tmp)
        .map_err(|error| format!("failed to create {}: {error}", tmp.display()))?;
    file.write_all(bytes)
        .map_err(|error| format!("failed to write {}: {error}", tmp.display()))?;
    file.sync_all()
        .map_err(|error| format!("failed to sync {}: {error}", tmp.display()))?;
    if path.exists() {
        let permissions = fs::metadata(path)
            .map_err(|error| format!("failed to inspect {}: {error}", path.display()))?
            .permissions();
        fs::set_permissions(&tmp, permissions)
            .map_err(|error| format!("failed to preserve permissions: {error}"))?;
    }
    fs::rename(&tmp, path).map_err(|error| format!("failed to install {}: {error}", path.display()))
}

fn latest_backup_marker(config_path: &Path) -> PathBuf {
    let file_name = config_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config.toml");
    config_path.with_file_name(format!("{file_name}.doctor-backup-latest"))
}

fn now_epoch_seconds() -> u64 {
    std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_secs())
                .unwrap_or(0)
        })
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn api_key_shape_heuristic() {
        assert!(auth::looks_like_api_key("exa-deadbeef"));
        assert!(auth::looks_like_api_key(
            "11111111-2222-3333-4444-555555555555"
        ));
        assert!(!auth::looks_like_api_key("svc-admin-key"));
        assert!(auth::looks_like_service_key("svc-admin-key"));
        assert!(auth::looks_like_service_key("service_admin_key"));
        assert!(!auth::looks_like_service_key("exa-deadbeef"));
    }
}
