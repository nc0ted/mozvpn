use anyhow::{bail, Result};
use std::process::Command;

fn enable_gnome(http_port: u16, socks_port: u16) -> Result<()> {
    let status = Command::new("gsettings")
        .args(["set", "org.gnome.system.proxy", "mode", "manual"])
        .status()?;
    if !status.success() {
        bail!("gsettings returned non-zero status");
    }
    let _ = Command::new("gsettings")
        .args(["set", "org.gnome.system.proxy.http", "host", "127.0.0.1"])
        .status();
    let _ = Command::new("gsettings")
        .args(["set", "org.gnome.system.proxy.http", "port", &http_port.to_string()])
        .status();
    let _ = Command::new("gsettings")
        .args(["set", "org.gnome.system.proxy.https", "host", "127.0.0.1"])
        .status();
    let _ = Command::new("gsettings")
        .args(["set", "org.gnome.system.proxy.https", "port", &http_port.to_string()])
        .status();
    let _ = Command::new("gsettings")
        .args(["set", "org.gnome.system.proxy.socks", "host", "127.0.0.1"])
        .status();
    let _ = Command::new("gsettings")
        .args(["set", "org.gnome.system.proxy.socks", "port", &socks_port.to_string()])
        .status();
    Ok(())
}

fn disable_gnome() -> Result<()> {
    let status = Command::new("gsettings")
        .args(["set", "org.gnome.system.proxy", "mode", "none"])
        .status()?;
    if !status.success() {
        bail!("gsettings disable returned non-zero status");
    }
    Ok(())
}

fn enable_kde(http_port: u16, socks_port: u16) -> Result<()> {
    let bin = if Command::new("kwriteconfig6").arg("--version").output().is_ok() {
        "kwriteconfig6"
    } else if Command::new("kwriteconfig5").arg("--version").output().is_ok() {
        "kwriteconfig5"
    } else {
        bail!("kwriteconfig not found");
    };

    let status = Command::new(bin)
        .args(["--file", "kioslaverc", "--group", "Proxy Settings", "--key", "ProxyType", "1"])
        .status()?;
    if !status.success() {
        bail!("kde proxy type configuration failed");
    }
    let _ = Command::new(bin)
        .args(["--file", "kioslaverc", "--group", "Proxy Settings", "--key", "httpProxy", &format!("http://127.0.0.1:{}", http_port)])
        .status();
    let _ = Command::new(bin)
        .args(["--file", "kioslaverc", "--group", "Proxy Settings", "--key", "httpsProxy", &format!("http://127.0.0.1:{}", http_port)])
        .status();
    let _ = Command::new(bin)
        .args(["--file", "kioslaverc", "--group", "Proxy Settings", "--key", "socksProxy", &format!("socks://127.0.0.1:{}", socks_port)])
        .status();
    Ok(())
}

fn disable_kde() -> Result<()> {
    let bin = if Command::new("kwriteconfig6").arg("--version").output().is_ok() {
        "kwriteconfig6"
    } else if Command::new("kwriteconfig5").arg("--version").output().is_ok() {
        "kwriteconfig5"
    } else {
        bail!("kwriteconfig not found");
    };

    let status = Command::new(bin)
        .args(["--file", "kioslaverc", "--group", "Proxy Settings", "--key", "ProxyType", "0"])
        .status()?;
    if !status.success() {
        bail!("kde proxy disable failed");
    }
    Ok(())
}

pub struct SystemProxy;

impl SystemProxy {
    pub fn enable(http_port: u16, socks_port: u16) -> Result<()> {
        crate::log_info!("Setting OS system proxy to HTTP :{}, SOCKS5 :{}", http_port, socks_port);
        if enable_gnome(http_port, socks_port).is_ok() {
            return Ok(());
        }
        if enable_kde(http_port, socks_port).is_ok() {
            return Ok(());
        }
        bail!("failed to set system proxy: neither GNOME nor KDE backend available")
    }

    pub fn disable() -> Result<()> {
        crate::log_info!("Disabling OS system proxy");
        let r_gnome = disable_gnome();
        let r_kde = disable_kde();
        if r_gnome.is_err() && r_kde.is_err() {
            bail!("failed to disable system proxy on Linux");
        }
        Ok(())
    }
}
