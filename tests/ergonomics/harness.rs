use std::process::Command;

pub struct CliRun {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

pub fn run_cli(args: &[&str]) -> CliRun {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_exa-agent"));
    cmd.args(args)
        .env("SOURCE_DATE_EPOCH", "1782777600")
        .env_remove("EXA_OUTPUT")
        .env_remove("EXA_API_KEY")
        .env_remove("EXA_SERVICE_KEY")
        .env_remove("EXA_ADMIN_BASE_URL")
        .env(
            "EXA_AGENT_CONFIG",
            std::env::temp_dir()
                .join(format!("exa-agent-hermetic-{}", std::process::id()))
                .join("config.toml"),
        )
        .env(
            "EXA_AGENT_CREDENTIALS",
            std::env::temp_dir()
                .join(format!("exa-agent-hermetic-{}", std::process::id()))
                .join("credentials.json"),
        )
        .env_remove("EXA_PROFILE");
    let output = cmd
        .output()
        .unwrap_or_else(|e| panic!("failed to run exa-agent {args:?}: {e}"));
    CliRun {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code().unwrap_or(-1),
    }
}

pub fn stdout_json(run: &CliRun) -> serde_json::Value {
    assert_eq!(
        run.exit_code, 0,
        "expected success\nstdout:\n{}\nstderr:\n{}",
        run.stdout, run.stderr
    );
    assert!(
        run.stderr.is_empty(),
        "success stderr was not empty: {}",
        run.stderr
    );
    serde_json::from_str(&run.stdout).expect("stdout JSON")
}

pub fn stderr_json(run: &CliRun) -> serde_json::Value {
    assert_ne!(run.exit_code, 0, "expected failure stdout:\n{}", run.stdout);
    assert!(
        run.stdout.is_empty(),
        "failure stdout must be empty: {}",
        run.stdout
    );
    serde_json::from_str(run.stderr.trim()).expect("stderr JSON")
}

pub fn error_json(args: &[&str]) -> serde_json::Value {
    let run = run_cli(args);
    let json = stderr_json(&run);
    assert_eq!(json["schema"], "exa.cli.error.v1");
    assert_eq!(json["ok"], false);
    json
}

pub fn ok_json(args: &[&str]) -> serde_json::Value {
    let json = stdout_json(&run_cli(args));
    assert_eq!(json["ok"], true);
    json
}
