use std::{path::Path, process::Command};

#[test]
fn config_show_renders_explicit_config_file() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = temp_dir.path().join("flowd.toml");
    write_config(
        &config_path,
        r#"
database_path = "./custom.db"
observed_folders = ["~/Inbox"]
intelligence_enabled = false
suggestion_min_usefulness_score = 0.4
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["--config", config_path.to_str().unwrap(), "config", "show"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains(&format!("source = \"{}\"", config_path.display())));
    assert!(stdout.contains("database_path = \"./custom.db\""));
    assert!(stdout.contains("observed_folders = [\"~/Inbox\"]"));
    assert!(stdout.contains("intelligence_enabled = false"));
    assert!(stdout.contains("suggestion_min_usefulness_score = 0.4"));
}

#[test]
fn config_validate_reports_success_for_defaults() {
    let temp_dir = tempfile::tempdir().unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["config", "validate"])
        .current_dir(temp_dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout, "Config is valid: built-in defaults\n");
}

#[test]
fn config_validate_fails_for_invalid_config_file() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = temp_dir.path().join("flowd.toml");
    write_config(
        &config_path,
        r#"
database_path = "./flowd.db"
observed_folders = []
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args([
            "--config",
            config_path.to_str().unwrap(),
            "config",
            "validate",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("invalid config"));
    assert!(stderr.contains("observed_folders must contain at least one path"));
}

fn write_config(path: &Path, contents: &str) {
    std::fs::write(path, contents.trim_start()).unwrap();
}
