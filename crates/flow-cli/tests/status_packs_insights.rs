use std::{fs, path::Path, process::Command};

#[test]
fn status_matches_doctor_output() {
    let temp_dir = tempfile::tempdir().unwrap();
    let watch_path = temp_dir.path().join("Inbox");
    fs::create_dir(&watch_path).unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    let config_path = temp_dir.path().join("flowd.toml");
    fs::write(
        &config_path,
        format!(
            "database_path = \"{}\"\nobserved_folders = [\"{}\"]\nintelligence_enabled = false\n",
            db_path.display(),
            watch_path.display(),
        ),
    )
    .unwrap();

    // Create DB via setup path: open through flowctl stats (opens and migrates).
    let _ = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["--config", config_path.to_str().unwrap(), "stats"])
        .output()
        .unwrap();

    let status = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["--config", config_path.to_str().unwrap(), "status"])
        .env("FLOWD_DOCTOR_DAEMON_RUNNING", "0")
        .output()
        .unwrap();
    let doctor = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["--config", config_path.to_str().unwrap(), "doctor"])
        .env("FLOWD_DOCTOR_DAEMON_RUNNING", "0")
        .output()
        .unwrap();

    assert!(status.status.success());
    assert!(doctor.status.success());
    assert_eq!(status.stdout, doctor.stdout);
    let stdout = String::from_utf8(status.stdout).unwrap();
    assert!(stdout.contains("intelligence layer: disabled"));
}

#[test]
fn packs_install_copies_validated_pack() {
    let temp_dir = tempfile::tempdir().unwrap();
    let home = temp_dir.path().join("home");
    let xdg = home.join(".config");
    fs::create_dir_all(&xdg).unwrap();

    let pack_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/packs/demo-pack");
    let output = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["packs", "install", pack_src.to_str().unwrap()])
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &xdg)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let installed = xdg.join("flowd/packs/demo.rename-downloads/workflow-pack.toml");
    assert!(installed.is_file(), "missing {}", installed.display());

    let list = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["packs", "list"])
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &xdg)
        .output()
        .unwrap();
    assert!(list.status.success());
    let stdout = String::from_utf8(list.stdout).unwrap();
    assert!(stdout.contains("demo.rename-downloads"));
}

#[test]
fn packs_search_and_install_from_local_registry() {
    let temp_dir = tempfile::tempdir().unwrap();
    let home = temp_dir.path().join("home");
    let xdg = home.join(".config");
    fs::create_dir_all(&xdg).unwrap();

    let registry = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/registry/index.toml");
    let search = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args([
            "packs",
            "search",
            "rename",
            "--registry",
            registry.to_str().unwrap(),
        ])
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &xdg)
        .output()
        .unwrap();
    assert!(
        search.status.success(),
        "{}",
        String::from_utf8_lossy(&search.stderr)
    );
    let search_stdout = String::from_utf8(search.stdout).unwrap();
    assert!(search_stdout.contains("demo.rename-downloads"));
    assert!(search_stdout.contains("flowd example registry"));

    let install = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args([
            "packs",
            "install",
            "demo.rename-downloads",
            "--registry",
            registry.to_str().unwrap(),
        ])
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &xdg)
        .output()
        .unwrap();
    assert!(
        install.status.success(),
        "{}",
        String::from_utf8_lossy(&install.stderr)
    );
    let installed = xdg.join("flowd/packs/demo.rename-downloads/workflow-pack.toml");
    assert!(installed.is_file(), "missing {}", installed.display());
}

#[test]
fn run_without_dry_run_is_blocked() {
    use flow_db::{
        open_database,
        repo::{insert_automation, insert_pattern, insert_suggestion},
    };

    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    let config_path = temp_dir.path().join("flowd.toml");
    let inbox = temp_dir.path().join("inbox");
    let archive = temp_dir.path().join("archive");
    fs::create_dir_all(&inbox).unwrap();
    fs::write(inbox.join("invoice-1.pdf"), "x").unwrap();

    let conn = open_database(&db_path).unwrap();
    let pattern_id = insert_pattern(
        &conn,
        "CreateFile:invoice",
        2,
        1000,
        "invoice workflow",
        "2026-03-12T12:00:00Z",
        1.0,
        0.9,
    )
    .unwrap();
    let suggestion_id = insert_suggestion(
        &conn,
        pattern_id,
        "Organize invoices",
        "2026-03-12T12:00:00Z",
        0.9,
    )
    .unwrap();
    let spec = format!(
        "id: invoice\ntrigger:\n  type: file_created\n  path: {}\n  extension: pdf\n  name_contains: invoice\nactions:\n  - type: Move\n    destination: {}\nsafety:\n  dry_run_first: true\n  undo_log: true\n",
        inbox.display(),
        archive.display()
    );
    insert_automation(
        &conn,
        suggestion_id,
        &spec,
        "active",
        "invoice",
        "2026-03-12T12:00:00Z",
    )
    .unwrap();

    fs::write(
        &config_path,
        format!(
            "database_path = \"{}\"\nobserved_folders = [\"{}\"]\n",
            db_path.display(),
            inbox.display()
        ),
    )
    .unwrap();

    let blocked = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["--config", config_path.to_str().unwrap(), "run", "1"])
        .output()
        .unwrap();
    assert!(!blocked.status.success());
    let stderr = String::from_utf8_lossy(&blocked.stderr);
    assert!(stderr.contains("dry-run"), "{stderr}");

    let dry = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["--config", config_path.to_str().unwrap(), "dry-run", "1"])
        .output()
        .unwrap();
    assert!(dry.status.success());

    let run = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["--config", config_path.to_str().unwrap(), "run", "1"])
        .output()
        .unwrap();
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
}
