//! Read-only diagnostics (arch §9). Offline by default; detectors never mutate state.

use std::path::PathBuf;

use serde::Serialize;

use crate::auth::{self, CredentialNamespace};
use crate::config::{self, Config};
use crate::error::CliError;
use crate::redaction;
use crate::registry::{BUILD_DATE, EMBEDDED_SPEC_SHA256, GIT_SHA, SPEC_VERSION, TARGET};

pub const DOCTOR_SCHEMA: &str = "exa.cli.doctor.v1";

pub const DETECTOR_IDS: &[&str] = &[
    "config.parse",
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
}

/// Injectable environment for tests and dispatch.
#[derive(Debug)]
pub struct DoctorCtx {
    pub config_path: PathBuf,
    pub config_load: Result<Config, CliError>,
    pub api_key: Option<String>,
    pub service_key: Option<String>,
    pub stdout_is_tty: bool,
    /// When set (tests), `spec.hash` compares against this instead of always passing.
    pub expected_spec_sha256: Option<String>,
}

impl DoctorCtx {
    pub fn from_process() -> Self {
        Self {
            config_path: config::config_path(),
            config_load: Config::load(),
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
}

impl DoctorReport {
    pub fn to_json(&self) -> serde_json::Value {
        let mut value = serde_json::to_value(self).unwrap_or(serde_json::Value::Null);
        redaction::scrub_json_value(&mut value);
        value
    }
}

pub fn run_doctor(options: &DoctorOptions, ctx: &DoctorCtx) -> DoctorReport {
    let mut findings = Vec::new();
    for id in DETECTOR_IDS {
        if !options.checks.is_empty() && !options.checks.iter().any(|c| c == id) {
            continue;
        }
        let finding = match *id {
            "config.parse" => detect_config_parse(ctx),
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
        findings.push(scrub_finding(finding));
    }

    let status = summarize_status(&findings);
    let ok = matches!(status, DoctorStatus::Healthy);
    DoctorReport {
        schema: DOCTOR_SCHEMA,
        ok,
        status,
        findings,
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

fn scrub_finding(mut finding: Finding) -> Finding {
    finding.message = redaction::scrub_text(&finding.message);
    finding.suggested_command = finding
        .suggested_command
        .as_deref()
        .map(redaction::scrub_text);
    finding
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

fn detect_auth_online(options: &DoctorOptions, _ctx: &DoctorCtx) -> Finding {
    if !options.online {
        return skipped_online("auth.online");
    }
    Finding {
        id: "auth.online",
        status: FindingStatus::Skip,
        category: "auth",
        message: "online auth probe not wired in this build".to_string(),
        suggested_command: Some("exa-agent auth test".to_string()),
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
    Finding {
        id: "connectivity",
        status: FindingStatus::Skip,
        category: "network",
        message: format!("connectivity check for `{base}` not wired in this build"),
        suggested_command: Some("exa-agent doctor --online".to_string()),
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
