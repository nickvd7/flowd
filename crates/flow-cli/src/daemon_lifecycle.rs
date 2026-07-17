use anyhow::{bail, Context, Result};
use flow_core::config::{expand_home, home_dir, LoadedConfig};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Duration,
};

const PID_FILE_NAME: &str = "flow-daemon.pid";
const SYSTEMD_UNIT_NAME: &str = "flow-daemon.service";
const LAUNCHD_LABEL: &str = "dev.flowd.daemon";

pub fn runtime_dir() -> PathBuf {
    if let Some(xdg) = env::var_os("XDG_RUNTIME_DIR") {
        return PathBuf::from(xdg).join("flowd");
    }
    expand_home("~/.flowd")
}

pub fn pid_file_path() -> PathBuf {
    runtime_dir().join(PID_FILE_NAME)
}

pub fn log_file_path() -> PathBuf {
    runtime_dir().join("flow-daemon.log")
}

pub fn daemon_status() -> Result<String> {
    let log = log_file_path();
    match read_pid()? {
        Some(pid) if process_is_running(pid) => Ok(format!(
            "running (pid {pid})\nLogs: {}",
            log.display()
        )),
        Some(pid) => {
            let _ = fs::remove_file(pid_file_path());
            Ok(format!(
                "not running (stale pid file for {pid} removed)\nLogs: {}",
                log.display()
            ))
        }
        None => Ok(format!("not running\nLogs: {}", log.display())),
    }
}

pub fn start_daemon(loaded: &LoadedConfig) -> Result<()> {
    if let Some(pid) = read_pid()? {
        if process_is_running(pid) {
            bail!("flow-daemon is already running (pid {pid})");
        }
        let _ = fs::remove_file(pid_file_path());
    }

    let daemon_bin = resolve_daemon_binary()?;
    let mut command = Command::new(&daemon_bin);
    if let flow_core::config::ConfigSource::File(path) = &loaded.source {
        command.arg("--config").arg(path);
    }

    fs::create_dir_all(runtime_dir())
        .with_context(|| format!("failed to create {}", runtime_dir().display()))?;
    let log_path = log_file_path();
    let log_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("failed to open log {}", log_path.display()))?;
    let log_err = log_file
        .try_clone()
        .context("failed to clone daemon log handle")?;

    let child = command
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_err))
        .spawn()
        .with_context(|| format!("failed to start {}", daemon_bin.display()))?;

    fs::write(pid_file_path(), child.id().to_string())
        .with_context(|| format!("failed to write {}", pid_file_path().display()))?;

    // Brief settle so status is meaningful for callers.
    thread::sleep(Duration::from_millis(150));
    if !process_is_running(child.id()) {
        let _ = fs::remove_file(pid_file_path());
        bail!(
            "flow-daemon exited immediately; inspect {}",
            log_path.display()
        );
    }

    println!(
        "Started flow-daemon (pid {})\nLogs: {}",
        child.id(),
        log_path.display()
    );
    Ok(())
}

pub fn stop_daemon() -> Result<()> {
    let Some(pid) = read_pid()? else {
        println!("flow-daemon is not running");
        return Ok(());
    };

    if !process_is_running(pid) {
        let _ = fs::remove_file(pid_file_path());
        println!("flow-daemon is not running (cleared stale pid file)");
        return Ok(());
    }

    let status = Command::new("kill")
        .arg(pid.to_string())
        .status()
        .context("failed to signal flow-daemon")?;
    if !status.success() {
        bail!("failed to stop flow-daemon (pid {pid})");
    }

    for _ in 0..20 {
        if !process_is_running(pid) {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }

    let _ = fs::remove_file(pid_file_path());
    println!("Stopped flow-daemon (pid {pid})");
    Ok(())
}

pub fn install_service(loaded: &LoadedConfig) -> Result<()> {
    let daemon_bin = resolve_daemon_binary()?;
    let config_arg = match &loaded.source {
        flow_core::config::ConfigSource::File(path) => {
            format!(" --config {}", path.display())
        }
        flow_core::config::ConfigSource::Default => String::new(),
    };

    if cfg!(target_os = "linux") {
        install_systemd_user_unit(&daemon_bin, &config_arg)?;
    } else if cfg!(target_os = "macos") {
        install_launchd_agent(&daemon_bin, &config_arg)?;
    } else {
        bail!("install-service currently supports Linux (systemd --user) and macOS (launchd)");
    }
    Ok(())
}

fn install_systemd_user_unit(daemon_bin: &Path, config_arg: &str) -> Result<()> {
    let unit_dir = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|home| home.join(".config")))
        .context("could not resolve XDG config home")?
        .join("systemd/user");
    fs::create_dir_all(&unit_dir)
        .with_context(|| format!("failed to create {}", unit_dir.display()))?;

    let unit_path = unit_dir.join(SYSTEMD_UNIT_NAME);
    let unit = format!(
        "[Unit]\n\
Description=flowd local observation daemon\n\
After=default.target\n\
\n\
[Service]\n\
Type=simple\n\
ExecStart={daemon}{config}\n\
Restart=on-failure\n\
RestartSec=2\n\
\n\
[Install]\n\
WantedBy=default.target\n",
        daemon = daemon_bin.display(),
        config = config_arg
    );
    fs::write(&unit_path, unit)
        .with_context(|| format!("failed to write {}", unit_path.display()))?;

    let _ = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status();
    println!("Wrote systemd user unit: {}", unit_path.display());
    println!("Enable with: systemctl --user enable --now {SYSTEMD_UNIT_NAME}");
    Ok(())
}

fn install_launchd_agent(daemon_bin: &Path, config_arg: &str) -> Result<()> {
    let agents = home_dir()
        .context("HOME is required for launchd install")?
        .join("Library/LaunchAgents");
    fs::create_dir_all(&agents).with_context(|| format!("failed to create {}", agents.display()))?;
    let plist_path = agents.join(format!("{LAUNCHD_LABEL}.plist"));

    let mut program_arguments = format!(
        "    <string>{}</string>\n",
        xml_escape(&daemon_bin.display().to_string())
    );
    if let Some(config_path) = config_arg.strip_prefix(" --config ") {
        program_arguments.push_str("    <string>--config</string>\n");
        program_arguments.push_str(&format!(
            "    <string>{}</string>\n",
            xml_escape(config_path)
        ));
    }

    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{LAUNCHD_LABEL}</string>
  <key>ProgramArguments</key>
  <array>
{program_arguments}  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
</dict>
</plist>
"#
    );
    fs::write(&plist_path, plist)
        .with_context(|| format!("failed to write {}", plist_path.display()))?;
    println!("Wrote launchd agent: {}", plist_path.display());
    println!("Load with: launchctl load {}", plist_path.display());
    Ok(())
}

fn resolve_daemon_binary() -> Result<PathBuf> {
    if let Ok(explicit) = env::var("FLOWD_DAEMON_BIN") {
        return Ok(PathBuf::from(explicit));
    }
    if let Ok(current) = env::current_exe() {
        if let Some(dir) = current.parent() {
            let candidate = dir.join("flow-daemon");
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Ok(PathBuf::from("flow-daemon"))
}

fn read_pid() -> Result<Option<u32>> {
    let path = pid_file_path();
    if !path.is_file() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let pid = raw
        .trim()
        .parse::<u32>()
        .with_context(|| format!("invalid pid file {}", path.display()))?;
    Ok(Some(pid))
}

fn process_is_running(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
