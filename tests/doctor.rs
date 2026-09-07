//! Doctor module tests (Wave 1C).

use exa_agent_cli::config::Config;
use exa_agent_cli::doctor::{
    doctor_exit_code, run_doctor, validate_check_ids, DoctorCtx, DoctorOptions, DoctorStatus,
    FindingStatus, OnlineProbes,
};
use exa_agent_cli::transport::AuthProbe;
use std::fs;
use std::path::PathBuf;

fn online_ctx(name: &str, probes: OnlineProbes) -> DoctorCtx {
    let path = temp_config_path(name);
    fs::write(&path, "base_url = \"https://api.exa.ai\"\n").unwrap();
    DoctorCtx {
        config_path: path.clone(),
        config_load: Config::load_from_path(&path),
        credentials_path: path.with_file_name("credentials.json"),
        state_dir: path.with_file_name("state"),
        api_key: Some("exa-test".to_string()),
        service_key: None,
        stdout_is_tty: false,
        expected_spec_sha256: None,
        online_probes: Some(probes),
    }
}

fn finding_status(report: &exa_agent_cli::doctor::DoctorReport, id: &str) -> FindingStatus {
    report
        .findings
        .iter()
        .find(|f| f.id == id)
        .unwrap_or_else(|| panic!("missing finding {id}"))
        .status
}

fn temp_config_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "exa-agent-doctor-test-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    exa_agent_cli::fsutil::create_dir_all_private(&dir).unwrap();
    dir.join("config.toml")
}

fn fixture_ctx(path: &std::path::Path) -> DoctorCtx {
    DoctorCtx {
        config_path: path.to_owned(),
        config_load: Config::load_from_path(path),
        credentials_path: path.with_file_name("credentials.json"),
        state_dir: path.with_file_name("state"),
        api_key: None,
        service_key: None,
        stdout_is_tty: false,
        expected_spec_sha256: None,
        online_probes: None,
    }
}

#[cfg(unix)]
#[test]
fn fix_and_undo_refuse_unavailable_lock_before_mutation() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    for undo in [false, true] {
        for symlinked in [false, true] {
            let path = temp_config_path(&format!("refuse-lock-{undo}-{symlinked}"));
            fs::write(&path, "retry=1\n").unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
            let lock = exa_agent_cli::fsutil::lock_path_for(&path);
            let sentinel = path.with_file_name("sentinel");
            fs::write(&sentinel, "untouched").unwrap();
            if symlinked {
                symlink(&sentinel, &lock).unwrap();
            } else {
                fs::create_dir(&lock).unwrap();
            }
            let options = DoctorOptions {
                fix: !undo,
                undo,
                checks: vec!["permissions.config".into()],
                ..Default::default()
            };
            let report = run_doctor(&options, &fixture_ctx(&path));
            assert_eq!(doctor_exit_code(&report), 4);
            assert_eq!(
                finding_status(&report, "config.lock"),
                FindingStatus::Refused
            );
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o644
            );
            assert_eq!(fs::read_to_string(&path).unwrap(), "retry=1\n");
            assert_eq!(fs::read_to_string(&sentinel).unwrap(), "untouched");
            assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 3);
            if symlinked {
                fs::remove_file(&lock).unwrap();
            } else {
                fs::remove_dir(&lock).unwrap();
            }
            let fixed = run_doctor(
                &DoctorOptions {
                    fix: true,
                    checks: vec!["permissions.config".into()],
                    ..Default::default()
                },
                &fixture_ctx(&path),
            );
            assert_eq!(doctor_exit_code(&fixed), 0);
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
            let before = fs::read(&path).unwrap();
            let planned = run_doctor(
                &DoctorOptions {
                    undo: true,
                    dry_run: true,
                    ..Default::default()
                },
                &fixture_ctx(&path),
            );
            assert_eq!(doctor_exit_code(&planned), 0);
            assert_eq!(
                planned.actions[0].status,
                exa_agent_cli::doctor::FixStatus::Planned
            );
            assert_eq!(fs::read(&path).unwrap(), before);
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
            let restored = run_doctor(
                &DoctorOptions {
                    undo: true,
                    ..Default::default()
                },
                &fixture_ctx(&path),
            );
            assert_eq!(doctor_exit_code(&restored), 0);
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o644
            );
        }
    }
}

#[test]
fn fix_dry_run_never_creates_missing_parent_or_lock() {
    let root_path = temp_config_path("dry-missing");
    let path = root_path.with_file_name("missing").join("config.toml");
    let ctx = fixture_ctx(&path);
    let options = DoctorOptions {
        fix: true,
        dry_run: true,
        checks: vec!["permissions.config".into()],
        ..Default::default()
    };
    run_doctor(&options, &ctx);
    assert!(!path.parent().unwrap().exists());
    assert_eq!(
        fs::read_dir(root_path.parent().unwrap()).unwrap().count(),
        0
    );
    fs::create_dir(path.parent().unwrap()).unwrap();
    fs::write(&path, "retry=1\n").unwrap();
    let before = fs::read(&path).unwrap();
    run_doctor(&options, &fixture_ctx(&path));
    assert_eq!(fs::read(&path).unwrap(), before);
    assert!(!exa_agent_cli::fsutil::lock_path_for(&path).exists());
}

#[test]
fn doctor_waits_for_config_lock_and_refreshes_the_snapshot() {
    let path = temp_config_path("config-lock-refresh");
    fs::write(&path, "retry = 1\n").unwrap();
    let ctx = fixture_ctx(&path);
    let guard = exa_agent_cli::fsutil::lock_exclusive(&path).unwrap();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        let report = run_doctor(
            &DoctorOptions {
                fix: true,
                checks: vec!["config.parse".into()],
                ..Default::default()
            },
            &ctx,
        );
        done_tx.send(report).unwrap();
    });
    started_rx.recv().unwrap();
    let early = done_rx.recv_timeout(std::time::Duration::from_millis(100));
    // Write as the existing lock owner, like a concurrent config transaction.
    fs::write(&path, "active_profile = \"missing\"\n").unwrap();
    drop(guard);
    assert!(
        early.is_err(),
        "doctor ran while another config transaction held the lock"
    );
    let report = done_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    worker.join().unwrap();
    assert_eq!(finding_status(&report, "config.parse"), FindingStatus::Fail);
    assert!(report.findings[0].message.contains("missing"));
}

#[cfg(unix)]
#[test]
fn permissions_state_reports_writable_directories_without_following_links() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let path = temp_config_path("state-directories");
    let ctx = fixture_ctx(&path);
    fs::create_dir_all(ctx.state_dir.join("spill")).unwrap();
    let outside = path.with_file_name("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("not-managed"), "private fixture").unwrap();
    fs::set_permissions(
        outside.join("not-managed"),
        fs::Permissions::from_mode(0o666),
    )
    .unwrap();
    symlink(&outside, ctx.state_dir.join("linked")).unwrap();
    let options = DoctorOptions {
        checks: vec!["permissions.state".into()],
        ..Default::default()
    };
    for mode in [0o755, 0o775] {
        for dir in [
            &ctx.state_dir,
            &ctx.state_dir.join("spill"),
            path.parent().unwrap(),
        ] {
            fs::set_permissions(dir, fs::Permissions::from_mode(mode)).unwrap();
        }
        let report = run_doctor(&options, &ctx);
        assert_eq!(doctor_exit_code(&report), if mode == 0o755 { 0 } else { 1 });
        let json = report.to_json().to_string();
        assert!(!json.contains("not-managed"));
        assert!(!json.contains("private fixture"));
        if mode == 0o775 {
            assert!(json.contains("0775"));
        }
        assert_eq!(
            fs::metadata(&ctx.state_dir).unwrap().permissions().mode() & 0o777,
            mode
        );
    }
    for dir in [
        &ctx.state_dir,
        &ctx.state_dir.join("spill"),
        path.parent().unwrap(),
    ] {
        fs::set_permissions(dir, fs::Permissions::from_mode(0o755)).unwrap();
    }
    // Each directory is independently sufficient; a state warning cannot mask
    // an omitted credential-directory check.
    for dir in [
        &ctx.state_dir,
        &ctx.state_dir.join("spill"),
        path.parent().unwrap(),
    ] {
        fs::set_permissions(dir, fs::Permissions::from_mode(0o775)).unwrap();
        let report = run_doctor(&options, &ctx);
        assert_eq!(doctor_exit_code(&report), 1);
        assert!(report.findings[0]
            .message
            .contains(&format!("{} (0775)", dir.display())));
        fs::set_permissions(dir, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(doctor_exit_code(&run_doctor(&options, &ctx)), 0);
    }
}

#[test]
fn doctor_healthy_when_config_and_key_ok() {
    let path = temp_config_path("healthy");
    fs::write(&path, "base_url = \"https://api.exa.ai\"\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let ctx = DoctorCtx {
        config_path: path.clone(),
        config_load: Config::load_from_path(&path),
        credentials_path: path.with_file_name("credentials.json"),
        state_dir: path.with_file_name("state"),
        api_key: Some("exa-test-key".to_string()),
        service_key: None,
        stdout_is_tty: false,
        expected_spec_sha256: None,
        online_probes: None,
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
        credentials_path: path.with_file_name("credentials.json"),
        state_dir: path.with_file_name("state"),
        api_key: None,
        service_key: None,
        stdout_is_tty: false,
        expected_spec_sha256: None,
        online_probes: None,
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
fn doctor_config_parse_message_preserves_exa_agent_cli_path() {
    let disk_path = temp_config_path("ok");
    fs::write(&disk_path, "base_url = \"https://api.exa.ai\"\n").unwrap();
    let load = Config::load_from_path(&disk_path);
    let path = PathBuf::from("/tmp/.config/exa-agent-cli/config.toml");
    let ctx = DoctorCtx {
        config_path: path.clone(),
        config_load: load,
        credentials_path: path.with_file_name("credentials.json"),
        state_dir: path.with_file_name("state"),
        api_key: None,
        service_key: None,
        stdout_is_tty: false,
        expected_spec_sha256: None,
        online_probes: None,
    };
    let report = run_doctor(
        &DoctorOptions {
            checks: vec!["config.parse".to_string()],
            ..DoctorOptions::default()
        },
        &ctx,
    );
    let json = report.to_json();
    let finding = json["findings"]
        .as_array()
        .and_then(|findings| {
            findings
                .iter()
                .find(|finding| finding["id"] == "config.parse")
        })
        .expect("config.parse finding");
    let message = finding["message"].as_str().expect("message");
    assert!(
        message.contains("exa-agent-cli"),
        "expected full config path segment, got: {message}"
    );
    assert!(!message.contains("<redacted>"));
}

#[test]
fn doctor_service_key_scope_finding() {
    let path = temp_config_path("scope");
    fs::write(&path, "base_url = \"https://api.exa.ai\"\n").unwrap();
    let ctx = DoctorCtx {
        config_path: path.clone(),
        config_load: Config::load_from_path(&path),
        credentials_path: path.with_file_name("credentials.json"),
        state_dir: path.with_file_name("state"),
        api_key: None,
        service_key: Some("exa-not-a-service-key".to_string()),
        stdout_is_tty: false,
        expected_spec_sha256: None,
        online_probes: None,
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
        credentials_path: path.with_file_name("credentials.json"),
        state_dir: path.with_file_name("state"),
        api_key: None,
        service_key: None,
        stdout_is_tty: false,
        expected_spec_sha256: None,
        online_probes: None,
    };
    let report = run_doctor(
        &DoctorOptions {
            online: false,
            checks: vec!["key.present".to_string()],
            ..DoctorOptions::default()
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
        credentials_path: path.with_file_name("credentials.json"),
        state_dir: path.with_file_name("state"),
        api_key: Some("exa-test".to_string()),
        service_key: None,
        stdout_is_tty: false,
        expected_spec_sha256: None,
        online_probes: None,
    };
    let report = run_doctor(&DoctorOptions::default(), &ctx);
    let json = report.to_json();
    assert_eq!(json["schema"], "exa.cli.doctor.v1");
    assert!(json["findings"].is_array());
}

#[test]
fn doctor_online_reports_reachable_host_and_accepted_credential() {
    let ctx = online_ctx(
        "online-ok",
        OnlineProbes {
            connectivity: Ok(404),
            auth: Some(Ok(AuthProbe::Accepted { status: 400 })),
        },
    );
    let report = run_doctor(
        &DoctorOptions {
            online: true,
            checks: vec![],
            ..DoctorOptions::default()
        },
        &ctx,
    );
    assert_eq!(finding_status(&report, "connectivity"), FindingStatus::Ok);
    assert_eq!(finding_status(&report, "auth.online"), FindingStatus::Ok);
}

#[test]
fn doctor_online_flags_unreachable_host_and_rejected_credential() {
    let ctx = online_ctx(
        "online-bad",
        OnlineProbes {
            connectivity: Err("dns failure".to_string()),
            auth: Some(Ok(AuthProbe::Rejected { status: 401 })),
        },
    );
    let report = run_doctor(
        &DoctorOptions {
            online: true,
            checks: vec![],
            ..DoctorOptions::default()
        },
        &ctx,
    );
    assert_eq!(report.status, DoctorStatus::Findings);
    assert_eq!(finding_status(&report, "connectivity"), FindingStatus::Fail);
    assert_eq!(finding_status(&report, "auth.online"), FindingStatus::Fail);
}

#[test]
fn doctor_online_auth_skips_when_no_credential_resolves() {
    let ctx = online_ctx(
        "online-nokey",
        OnlineProbes {
            connectivity: Ok(404),
            auth: None,
        },
    );
    let report = run_doctor(
        &DoctorOptions {
            online: true,
            checks: vec![],
            ..DoctorOptions::default()
        },
        &ctx,
    );
    assert_eq!(finding_status(&report, "auth.online"), FindingStatus::Skip);
}

#[test]
fn doctor_skips_online_detectors_by_default() {
    let path = temp_config_path("offline");
    fs::write(&path, "base_url = \"https://api.exa.ai\"\n").unwrap();
    let ctx = DoctorCtx {
        config_path: path.clone(),
        config_load: Config::load_from_path(&path),
        credentials_path: path.with_file_name("credentials.json"),
        state_dir: path.with_file_name("state"),
        api_key: Some("exa-test".to_string()),
        service_key: None,
        stdout_is_tty: false,
        expected_spec_sha256: None,
        online_probes: None,
    };
    let report = run_doctor(&DoctorOptions::default(), &ctx);
    let connectivity = report
        .findings
        .iter()
        .find(|f| f.id == "connectivity")
        .expect("connectivity finding");
    assert_eq!(connectivity.status, FindingStatus::Skip);
}
