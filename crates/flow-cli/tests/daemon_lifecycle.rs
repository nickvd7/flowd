use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::Duration,
};

#[test]
fn daemon_start_status_clears_stale_pid_and_restarts() {
    let runtime = tempfile::tempdir().unwrap();
    let fake_daemon = runtime.path().join("fake-daemon.sh");
    fs::write(
        &fake_daemon,
        "#!/bin/sh\ntrap 'exit 0' TERM INT\nwhile true; do sleep 1; done\n",
    )
    .unwrap();
    Command::new("chmod")
        .args(["+x", fake_daemon.to_str().unwrap()])
        .status()
        .unwrap();

    let xdg = runtime.path().join("xdg");
    fs::create_dir_all(&xdg).unwrap();

    let start = flowctl_daemon(&["daemon", "start"], &xdg, &fake_daemon);
    assert!(
        start.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&start.stderr)
    );
    let start_out = String::from_utf8_lossy(&start.stdout);
    assert!(start_out.contains("Started flow-daemon"));
    assert!(start_out.contains("Logs:"));

    let status = flowctl_daemon(&["daemon", "status"], &xdg, &fake_daemon);
    assert!(status.status.success());
    let status_out = String::from_utf8_lossy(&status.stdout);
    assert!(status_out.contains("running (pid "));

    let pid = read_pid(&xdg);
    Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status()
        .unwrap();
    wait_until_dead(pid);

    let stale = flowctl_daemon(&["daemon", "status"], &xdg, &fake_daemon);
    assert!(stale.status.success());
    let stale_out = String::from_utf8_lossy(&stale.stdout);
    assert!(
        stale_out.contains("stale pid"),
        "unexpected status after kill: {stale_out}"
    );

    let restart = flowctl_daemon(&["daemon", "start"], &xdg, &fake_daemon);
    assert!(
        restart.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&restart.stderr)
    );
    let running = flowctl_daemon(&["daemon", "status"], &xdg, &fake_daemon);
    assert!(String::from_utf8_lossy(&running.stdout).contains("running (pid "));

    let stop = flowctl_daemon(&["daemon", "stop"], &xdg, &fake_daemon);
    assert!(stop.status.success());
    let stopped = flowctl_daemon(&["daemon", "status"], &xdg, &fake_daemon);
    assert!(String::from_utf8_lossy(&stopped.stdout).contains("not running"));
}

#[test]
fn service_templates_declare_restart_policy() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let systemd = fs::read_to_string(root.join("contrib/systemd/flow-daemon.service")).unwrap();
    assert!(systemd.contains("Restart=on-failure"));
    assert!(systemd.contains("RestartSec=2"));

    let launchd =
        fs::read_to_string(root.join("contrib/launchd/dev.flowd.daemon.plist")).unwrap();
    assert!(launchd.contains("<key>KeepAlive</key>"));
    assert!(launchd.contains("<true/>"));
}

fn flowctl_daemon(args: &[&str], xdg: &std::path::Path, daemon_bin: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_flowctl"))
        .args(args)
        .env("XDG_RUNTIME_DIR", xdg)
        .env("FLOWD_DAEMON_BIN", daemon_bin)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap()
}

fn read_pid(xdg: &std::path::Path) -> u32 {
    let path = xdg.join("flowd/flow-daemon.pid");
    fs::read_to_string(path)
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}

fn wait_until_dead(pid: u32) {
    for _ in 0..50 {
        let alive = Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        if !alive {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("pid {pid} did not exit after SIGKILL");
}
