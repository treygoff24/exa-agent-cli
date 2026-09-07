//! Multi-agent safety for the CLI's managed state: locked read-modify-write, private modes
//! independent of umask, and path resolution that cannot follow a process's working directory.
//!
//! Several exa-agent processes routinely run at once. Everything here uses throwaway homes and
//! fake keys; nothing reaches the network.

mod support;

use std::path::Path;
use std::process::{Command, Stdio};

use support::{isolated_command, scratch_dir};

const FAKE_API_KEY: &str = "test-key-abcdef12";

fn config_path(home: &Path) -> std::path::PathBuf {
    home.join("config.toml")
}

/// Eight agents each add their own profile at the same moment. Every one must survive.
///
/// The old `load → mutate → save` sequence held no lock: each process read the pre-existing
/// file, added one key, and wrote a full replacement, so all but the last writer vanished. A
/// unique temp name does not help — every one of those writes is individually well-formed.
#[test]
fn concurrent_config_writers_do_not_lose_each_others_keys() {
    let home = scratch_dir("config-race-home");
    // Seed the file so every writer starts from a real read-modify-write.
    let seed = isolated_command(&home)
        .args(["config", "set", "retry", "1", "--compact"])
        .output()
        .expect("seed config");
    assert!(seed.status.success());

    const WRITERS: usize = 8;
    let children: Vec<_> = (0..WRITERS)
        .map(|i| {
            isolated_command(&home)
                .args([
                    "config",
                    "profiles",
                    "create",
                    &format!("agent{i}"),
                    "--compact",
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .expect("spawn concurrent config writer")
        })
        .collect();
    for child in children {
        let output = child.wait_with_output().expect("await config writer");
        assert!(
            output.status.success(),
            "a concurrent config write failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let listed = isolated_command(&home)
        .args(["config", "profiles", "list", "--compact"])
        .output()
        .expect("list profiles");
    let value: serde_json::Value = serde_json::from_slice(&listed.stdout).expect("profiles JSON");
    let profiles = value["data"]["profiles"]
        .as_object()
        .expect("profiles object");
    for i in 0..WRITERS {
        assert!(
            profiles.contains_key(&format!("agent{i}")),
            "profile agent{i} was lost to a concurrent write; kept: {:?}",
            profiles.keys().collect::<Vec<_>>()
        );
    }
    assert!(config_path(&home).exists());
}

/// Managed files are private from creation, whatever umask the caller happens to have.
#[cfg(unix)]
#[test]
fn managed_files_are_private_under_a_permissive_umask() {
    use std::os::unix::fs::PermissionsExt;

    let home = scratch_dir("umask-home");
    let credentials = home.join("credentials.json");
    let config = home.join("config.toml");
    let state = home.join("state");

    // `umask 000` is the worst case: without an explicit creation mode these land 0666/0777.
    let script = format!(
        "umask 000; \
         printf '%s' '{FAKE_API_KEY}' | '{bin}' auth login --compact >/dev/null && \
         '{bin}' config set retry 4 --compact >/dev/null",
        bin = env!("CARGO_BIN_EXE_exa-agent")
    );
    let output = Command::new("sh")
        .arg("-c")
        .arg(&script)
        .env_remove("EXA_API_KEY")
        .env_remove("EXA_SERVICE_KEY")
        .env_remove("EXA_PROFILE")
        .env_remove("EXA_OUTPUT")
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("XDG_STATE_HOME")
        .env("HOME", &home)
        .env("EXA_AGENT_NO_NETWORK", "1")
        .env("EXA_AGENT_CONFIG", &config)
        .env("EXA_AGENT_CREDENTIALS", &credentials)
        .env("EXA_AGENT_STATE", &state)
        .output()
        .expect("run under a permissive umask");
    assert!(
        output.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    for path in [&credentials, &config] {
        let mode = std::fs::metadata(path)
            .expect("managed file")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            mode,
            0o600,
            "{} was created world-readable under umask 000",
            path.display()
        );
    }
}

/// A newly created managed state directory is 0700, and no temp file is left behind.
#[cfg(unix)]
#[test]
fn spilled_payloads_are_private_and_leave_no_temp_files() {
    use std::os::unix::fs::PermissionsExt;

    let home = scratch_dir("spill-home");
    let state = home.join("state");
    let filler = "y".repeat(4096);
    let server = support::TestServer::start(vec![support::Reply::json(&format!(
        r#"{{"requestId":"req-spill","results":[{{"id":"1","text":"{filler}"}}]}}"#
    ))]);
    let script = format!(
        "umask 000; exec '{bin}' search spill --api-key '{FAKE_API_KEY}' --base-url '{base}' \
         --max-output-bytes 64 --json",
        bin = env!("CARGO_BIN_EXE_exa-agent"),
        base = server.base_url
    );
    let output = support::isolated_program(&home, "sh")
        .arg("-c")
        .arg(&script)
        .env_remove("EXA_AGENT_NO_NETWORK")
        .output()
        .expect("run a spilling command under a permissive umask");
    server.finish();
    assert!(
        output.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let spill_dir = state.join("spill");
    let mode = std::fs::metadata(&spill_dir)
        .expect("spill directory")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o700, "spill directory was created group-readable");

    let entries: Vec<_> = std::fs::read_dir(&spill_dir)
        .expect("read spill dir")
        .map(|entry| entry.expect("spill entry").path())
        .collect();
    assert!(!entries.is_empty(), "the ceiling did not spill anything");
    for path in &entries {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            !name.ends_with(".tmp"),
            "a temporary spill file survived: {name}"
        );
        let mode = std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "spill file {name} is not private");
    }
}

/// Relative `XDG_*` values are ignored; the CLI falls back to `$HOME` instead of writing state
/// relative to whatever directory the agent happened to be in.
#[test]
fn relative_xdg_values_are_ignored_for_config_and_state() {
    let home = scratch_dir("xdg-home");
    let cwd = scratch_dir("xdg-cwd");

    let run = |xdg_config: &str, xdg_state: &str| {
        let make_command = || {
            let mut command = isolated_command(&home);
            command
                .current_dir(&cwd)
                .env_remove("EXA_AGENT_CONFIG")
                .env_remove("EXA_AGENT_STATE")
                .env("XDG_CONFIG_HOME", xdg_config)
                .env("XDG_STATE_HOME", xdg_state);
            command
        };
        let output = make_command()
            .args(["config", "path", "--compact"])
            .output()
            .unwrap();
        let state = make_command()
            .args(["doctor", "--check", "permissions.state", "--json"])
            .output()
            .unwrap();
        assert!(
            state.status.success(),
            "{}",
            String::from_utf8_lossy(&state.stdout)
        );
        let expected_state = if Path::new(xdg_state).is_absolute() {
            Path::new(xdg_state).to_owned()
        } else {
            home.join(".local/state")
        }
        .join("exa-agent-cli");
        let evidence: serde_json::Value = serde_json::from_slice(&state.stdout).unwrap();
        assert!(
            evidence["findings"][0]["message"]
                .as_str()
                .unwrap()
                .contains(expected_state.to_str().unwrap()),
            "{evidence}"
        );
        assert!(
            output.status.success(),
            "stderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("config path JSON");
        value["path"]
            .as_str()
            .or_else(|| value["data"]["path"].as_str())
            .or_else(|| value["configPath"].as_str())
            .unwrap_or_else(|| panic!("no path field in {value}"))
            .to_string()
    };

    let expected = home
        .join(".config")
        .join("exa-agent-cli")
        .join("config.toml")
        .display()
        .to_string();
    assert_eq!(run("relative/config", "relative/state"), expected);
    assert_eq!(run("", ""), expected);

    // An absolute value is still honored.
    let absolute = scratch_dir("xdg-absolute");
    assert_eq!(
        run(absolute.to_str().unwrap(), absolute.to_str().unwrap()),
        absolute
            .join("exa-agent-cli")
            .join("config.toml")
            .display()
            .to_string()
    );
}

/// `doctor` reports a group- or world-readable managed-state file, and reports only its path
/// and mode — never anything from inside it.
#[cfg(unix)]
#[test]
fn doctor_reports_permissive_managed_state_without_exposing_contents() {
    use std::os::unix::fs::PermissionsExt;

    let home = scratch_dir("doctor-state-home");
    let spill = home.join("state").join("spill");
    std::fs::create_dir_all(&spill).expect("create spill dir");
    let leaked = spill.join("leaky.json");
    std::fs::write(&leaked, r#"{"secretish":"do-not-print-me"}"#).expect("write spill file");
    std::fs::set_permissions(&leaked, std::fs::Permissions::from_mode(0o644))
        .expect("make the spill file permissive");

    let output = isolated_command(&home)
        .args(["doctor", "--check", "permissions.state", "--json"])
        .output()
        .expect("run doctor");
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("doctor JSON");
    let finding = report["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .find(|f| f["id"] == "permissions.state")
        .expect("permissions.state finding");
    assert_eq!(finding["status"], "warn", "{report}");
    let message = finding["message"].as_str().unwrap_or_default();
    assert!(message.contains("leaky.json"), "{message}");
    assert!(message.contains("0644"), "{message}");
    assert!(
        !message.contains("do-not-print-me"),
        "doctor leaked file contents: {message}"
    );
}
