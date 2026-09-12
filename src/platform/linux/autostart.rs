use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

fn autostart_desktop_path() -> Result<PathBuf> {
    let config_dir = dirs::config_dir().context("failed to determine user config dir")?;
    Ok(config_dir.join("autostart").join("outfox.desktop"))
}

fn systemd_service_path() -> Result<PathBuf> {
    let config_dir = dirs::config_dir().context("failed to determine user config dir")?;
    Ok(config_dir.join("systemd").join("user").join("outfox.service"))
}

fn systemd_wants_path() -> Result<PathBuf> {
    let config_dir = dirs::config_dir().context("failed to determine user config dir")?;
    Ok(config_dir
        .join("systemd")
        .join("user")
        .join("default.target.wants")
        .join("outfox.service"))
}

pub fn enable_autostart() -> Result<()> {
    let app_path = std::env::current_exe().context("failed to determine current exe path")?;

    let desktop_path = autostart_desktop_path()?;
    if let Some(parent) = desktop_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let desktop_content = format!(
        "[Desktop Entry]\nType=Application\nName=outfox\nExec={} --minimized\nIcon=outfox\nTerminal=false\nCategories=Network;\nX-systemd-skip=true\n",
        app_path.display()
    );
    fs::write(&desktop_path, desktop_content)?;

    let service_path = systemd_service_path()?;
    if let Some(parent) = service_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let service_content = format!(
        "[Unit]\nDescription=outfox\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\nType=simple\nExecStart={} --minimized\nRestart=on-failure\nRestartSec=5\n\n[Install]\nWantedBy=default.target\n",
        app_path.display()
    );
    fs::write(&service_path, service_content)?;

    let wants_path = systemd_wants_path()?;
    if let Some(parent) = wants_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if wants_path.exists() {
        let _ = fs::remove_file(&wants_path);
    }
    #[cfg(unix)]
    let _ = std::os::unix::fs::symlink(&service_path, &wants_path);

    let _ = std::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .output();

    Ok(())
}

pub fn disable_autostart() -> Result<()> {
    let desktop_path = autostart_desktop_path()?;
    if desktop_path.exists() {
        let _ = fs::remove_file(desktop_path);
    }

    let wants_path = systemd_wants_path()?;
    if wants_path.exists() {
        let _ = fs::remove_file(wants_path);
    }

    let service_path = systemd_service_path()?;
    if service_path.exists() {
        let _ = fs::remove_file(service_path);
    }

    let _ = std::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .output();

    Ok(())
}

pub fn is_autostart_enabled() -> bool {
    let desktop_exists = autostart_desktop_path()
        .map(|p| p.exists())
        .unwrap_or(false);
    let service_exists = systemd_wants_path()
        .map(|p| p.exists())
        .unwrap_or(false);
    desktop_exists || service_exists
}
