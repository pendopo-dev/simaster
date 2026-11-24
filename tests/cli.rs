use assert_cmd::Command;
use predicates::prelude::*;

fn simaster() -> Command {
    Command::new(env!("CARGO_BIN_EXE_simaster"))
}

#[test]
fn help_lists_top_level_commands() {
    simaster()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("auth"))
        .stdout(predicate::str::contains("profile"))
        .stdout(predicate::str::contains("doctor"))
        .stdout(predicate::str::contains("completions"));
}

#[test]
fn doctor_json_has_stable_envelope_and_version() {
    let output = simaster()
        .args(["doctor", "--json"])
        .output()
        .expect("doctor should run");

    assert!(output.status.success(), "doctor failed: {output:?}");
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("doctor output must be valid JSON");

    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["data"]["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(value["data"]["auth"]["status"], "not_checked");
    assert!(value["data"]["auth"]["provider"].is_null());
    assert!(value["meta"]["generated_at_unix"].is_u64());
}

#[test]
fn auth_login_help_exposes_browser_methods_without_vnext() {
    simaster()
        .args(["auth", "login", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("auto"))
        .stdout(predicate::str::contains("browser"))
        .stdout(predicate::str::contains("web"))
        .stdout(predicate::str::contains("vnext").not());
}

#[test]
fn auth_login_requires_registered_client_id() {
    simaster()
        .args(["auth", "login", "--method", "browser"])
        .env_remove("SIMASTER_CLIENT_ID")
        .assert()
        .code(1)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("CONFIG: set SIMASTER_CLIENT_ID"));
}

#[test]
fn completions_bash_is_generated() {
    simaster()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("simaster"));
}
