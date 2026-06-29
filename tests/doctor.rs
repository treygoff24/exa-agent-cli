//! Doctor module tests (Wave 1C).

use exa_agent_cli::config::Config;
use exa_agent_cli::doctor::{
    doctor_exit_code, run_doctor, validate_check_ids, DoctorCtx, DoctorOptions, DoctorStatus,
    FindingStatus,
};
use std::fs;
use std::path::PathBuf;

fn temp_config_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "exa-agent-doctor-test-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir.join("config.toml")
}

#[test]
fn doctor_healthy_when_config_and_key_ok() {
    let path = temp_config_path("healthy");
    fs::write(&path, "base_url = \"https://api.exa.ai\"\n").unwrap();
    let ctx = DoctorCtx {
        config_path: path.clone(),
        config_load: Config::load_from_path(&path),
        api_key: Some("exa-test-key".to_string()),
        service_key: None,
        stdout_is_tty: false,
        expected_spec_sha256: None,
    };
    let report = run_doctor(&DoctorOptions::default(), &ctx);
    assert_eq!(report.status, DoctorStatus::Healthy);
    assert_eq!(doctor_exit_code(&report), 0);
    assert!(report.ok);
}

#[test]
fn doctor_findings_when_config_malformed() {
    let path = temp_config_path("bad");
    fs::write(&path, "not = valid toml [[[\n").unwrap();
    let load = Config::load_from_path(&path);
    let ctx = DoctorCtx {
        config_path: path.clone(),
        config_load: load,
        api_key: None,
        service_key: None,
        stdout_is_tty: false,
        expected_spec_sha256: None,
    };
    let report = run_doctor(&DoctorOptions::default(), &ctx);
    assert_eq!(report.status, DoctorStatus::Findings);
    assert_eq!(doctor_exit_code(&report), 1);
    let parse = report
        .findings
        .iter()
        .find(|f| f.id == "config.parse")
        .expect("config.parse finding");
    assert_eq!(parse.status, FindingStatus::Fail);
}

#[test]
fn doctor_service_key_scope_finding() {
    let path = temp_config_path("scope");
    fs::write(&path, "base_url = \"https://api.exa.ai\"\n").unwrap();
    let ctx = DoctorCtx {
        config_path: path.clone(),
        config_load: Config::load_from_path(&path),
        api_key: None,
        service_key: Some("exa-not-a-service-key".to_string()),
        stdout_is_tty: false,
        expected_spec_sha256: None,
    };
    let report = run_doctor(&DoctorOptions::default(), &ctx);
    assert_eq!(report.status, DoctorStatus::Findings);
    assert_eq!(doctor_exit_code(&report), 1);
}

#[test]
fn doctor_warn_findings_make_report_non_healthy() {
    let path = temp_config_path("warn");
    fs::write(&path, "base_url = \"https://api.exa.ai\"\n").unwrap();
    let ctx = DoctorCtx {
        config_path: path.clone(),
        config_load: Config::load_from_path(&path),
        api_key: None,
        service_key: None,
        stdout_is_tty: false,
        expected_spec_sha256: None,
    };
    let report = run_doctor(
        &DoctorOptions {
            online: false,
            checks: vec!["key.present".to_string()],
        },
        &ctx,
    );
    assert_eq!(report.status, DoctorStatus::Findings);
    assert_eq!(doctor_exit_code(&report), 1);
    assert!(!report.ok);
    assert_eq!(report.findings[0].status, FindingStatus::Warn);
}

#[test]
fn doctor_unknown_check_ids_are_rejected() {
    let err = validate_check_ids(&["key.presnt".to_string()]).unwrap_err();
    assert_eq!(err.diag().code, "invalid_value");
}

#[test]
fn doctor_report_serializes_contract_fields() {
    let path = temp_config_path("json");
    fs::write(&path, "base_url = \"https://api.exa.ai\"\n").unwrap();
    let ctx = DoctorCtx {
        config_path: path.clone(),
        config_load: Config::load_from_path(&path),
        api_key: Some("exa-test".to_string()),
        service_key: None,
        stdout_is_tty: false,
        expected_spec_sha256: None,
    };
    let report = run_doctor(&DoctorOptions::default(), &ctx);
    let json = report.to_json();
    assert_eq!(json["schema"], "exa.cli.doctor.v1");
    assert!(json["findings"].is_array());
}

#[test]
fn doctor_skips_online_detectors_by_default() {
    let path = temp_config_path("offline");
    fs::write(&path, "base_url = \"https://api.exa.ai\"\n").unwrap();
    let ctx = DoctorCtx {
        config_path: path.clone(),
        config_load: Config::load_from_path(&path),
        api_key: Some("exa-test".to_string()),
        service_key: None,
        stdout_is_tty: false,
        expected_spec_sha256: None,
    };
    let report = run_doctor(&DoctorOptions::default(), &ctx);
    let connectivity = report
        .findings
        .iter()
        .find(|f| f.id == "connectivity")
        .expect("connectivity finding");
    assert_eq!(connectivity.status, FindingStatus::Skip);
}
