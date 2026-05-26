use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use mempal::core::db::Database;
use serde_json::Value;
use tempfile::TempDir;

fn mempal_bin() -> String {
    env!("CARGO_BIN_EXE_mempal").to_string()
}

fn run_mempal(home: &TempDir, args: &[&str]) -> std::process::Output {
    Command::new(mempal_bin())
        .args(args)
        .env("HOME", home.path())
        .output()
        .expect("run mempal")
}

fn run_mempal_with_path(home: &TempDir, args: &[&str], path_value: &str) -> std::process::Output {
    Command::new(mempal_bin())
        .args(args)
        .env("HOME", home.path())
        .env("PATH", path_value)
        .output()
        .expect("run mempal")
}

fn stdout(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn assert_success(output: &std::process::Output) {
    assert!(
        output.status.success(),
        "command failed\nstdout:\n{}\nstderr:\n{}",
        stdout(output),
        stderr(output)
    );
}

fn palace_db_path(home: &TempDir) -> PathBuf {
    home.path().join(".mempal/palace.db")
}

fn install_fake_path_mempal(home: &TempDir) -> String {
    let bin_dir = home.path().join("fake-bin");
    fs::create_dir_all(&bin_dir).expect("create fake bin");
    let fake = bin_dir.join("mempal");
    fs::write(&fake, "#!/bin/sh\nprintf 'mempal 0.0.0\\n'\n").expect("write fake mempal");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&fake).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&fake, perms).unwrap();
    }
    let old_path = std::env::var("PATH").unwrap_or_default();
    format!("{}:{old_path}", bin_dir.display())
}

#[test]
fn test_cli_doctor_json_reports_schema_and_path() {
    let home = TempDir::new().expect("home");
    fs::create_dir_all(home.path().join(".mempal")).expect("create mempal home");
    let db = Database::open(&palace_db_path(&home)).expect("open db");
    assert_eq!(db.schema_version().expect("schema"), 9);
    drop(db);
    let path_value = install_fake_path_mempal(&home);

    let output = run_mempal_with_path(&home, &["doctor", "--format", "json"], &path_value);
    assert_success(&output);
    let value: Value = serde_json::from_str(&stdout(&output)).expect("doctor json");
    assert_eq!(
        value["current_version"].as_str(),
        Some(env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(value["supported_schema_version"], 9);
    assert_eq!(value["db"]["exists"], true);
    assert_eq!(value["db"]["schema_version"], 9);
    assert_eq!(value["install"]["path_matches_current_exe"], false);
    assert!(
        value["warnings"]
            .as_array()
            .expect("warnings")
            .iter()
            .any(|warning| warning.as_str().unwrap_or_default().contains("PATH"))
    );
}

#[test]
fn test_cli_doctor_plain_no_db_is_read_only() {
    let home = TempDir::new().expect("home");
    let output = run_mempal(&home, &["doctor", "--format", "plain"]);
    assert_success(&output);
    let out = stdout(&output);
    assert!(out.contains("db_exists=false"), "{out}");
    assert!(!palace_db_path(&home).exists());
}

#[test]
fn test_cli_doctor_rejects_invalid_format() {
    let home = TempDir::new().expect("home");
    let output = run_mempal(&home, &["doctor", "--format", "yaml"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("unsupported doctor format"));
    assert!(!palace_db_path(&home).exists());
}

#[test]
fn test_cli_maintenance_guided_run_json() {
    let home = TempDir::new().expect("home");
    fs::create_dir_all(home.path().join(".mempal")).expect("create mempal home");
    Database::open(&palace_db_path(&home)).expect("open db");

    let output = run_mempal(&home, &["maintenance", "guided-run", "--format", "json"]);
    assert_success(&output);
    let value: Value = serde_json::from_str(&stdout(&output)).expect("guided run json");
    assert_eq!(value["writes"], false);
    let commands = value["steps"]
        .as_array()
        .expect("steps")
        .iter()
        .filter_map(|step| step["command"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(commands.contains("research-validate-plan"), "{commands}");
    assert!(commands.contains("adoption review"), "{commands}");
    assert!(commands.contains("cowork-doctor"), "{commands}");
}

#[test]
fn test_cli_maintenance_guided_run_plain() {
    let home = TempDir::new().expect("home");
    fs::create_dir_all(home.path().join(".mempal")).expect("create mempal home");
    Database::open(&palace_db_path(&home)).expect("open db");

    let output = run_mempal(&home, &["maintenance", "guided-run", "--format", "plain"]);
    assert_success(&output);
    let out = stdout(&output);
    assert!(out.contains("Guided Maintenance Run"), "{out}");
    assert!(out.contains("mempal phase3 adoption review"), "{out}");
    assert!(out.contains("mempal cowork-capture"), "{out}");
}

#[test]
fn test_cli_maintenance_guided_run_rejects_invalid_format() {
    let home = TempDir::new().expect("home");
    fs::create_dir_all(home.path().join(".mempal")).expect("create mempal home");
    Database::open(&palace_db_path(&home)).expect("open db");

    let output = run_mempal(&home, &["maintenance", "guided-run", "--format", "yaml"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("unsupported maintenance guided-run format"));
}

#[test]
fn test_cli_release_readiness_json() {
    let home = TempDir::new().expect("home");
    let output = Command::new(mempal_bin())
        .args(["release-readiness", "--format", "json"])
        .env("HOME", home.path())
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")))
        .output()
        .expect("run mempal");
    assert_success(&output);
    let value: Value = serde_json::from_str(&stdout(&output)).expect("release readiness json");
    assert_eq!(value["writes"], false);
    let check_names = value["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .filter_map(|check| check["name"].as_str())
        .collect::<Vec<_>>();
    assert!(check_names.contains(&"cargo-metadata"), "{check_names:?}");
    assert!(
        check_names.contains(&"spec-plan-inventory"),
        "{check_names:?}"
    );
    assert!(
        value["recommended_commands"]
            .as_array()
            .expect("commands")
            .iter()
            .any(|command| command
                .as_str()
                .unwrap_or_default()
                .contains("cargo package"))
    );
}

#[test]
fn test_cli_release_readiness_plain() {
    let home = TempDir::new().expect("home");
    let output = Command::new(mempal_bin())
        .args(["release-readiness", "--format", "plain"])
        .env("HOME", home.path())
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")))
        .output()
        .expect("run mempal");
    assert_success(&output);
    let out = stdout(&output);
    assert!(out.contains("Release Readiness"), "{out}");
    assert!(out.contains("cargo package"), "{out}");
    assert!(out.contains("mempal doctor"), "{out}");
}

#[test]
fn test_cli_release_readiness_rejects_invalid_format() {
    let home = TempDir::new().expect("home");
    let output = Command::new(mempal_bin())
        .args(["release-readiness", "--format", "yaml"])
        .env("HOME", home.path())
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")))
        .output()
        .expect("run mempal");
    assert!(!output.status.success());
    assert!(stderr(&output).contains("unsupported release-readiness format"));
}
