use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

fn plist_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("failed to determine user home dir")?;
    Ok(home.join("Library").join("LaunchAgents").join("com.nc0ted.outfox.plist"))
}

pub fn enable_autostart() -> Result<()> {
    let app_path = std::env::current_exe().context("failed to determine current exe path")?;
    let path = plist_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.nc0ted.outfox</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
        <string>--minimized</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>"#,
        app_path.display()
    );
    fs::write(&path, content)?;
    Ok(())
}

pub fn disable_autostart() -> Result<()> {
    let path = plist_path()?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

pub fn is_autostart_enabled() -> bool {
    plist_path().map(|p| p.exists()).unwrap_or(false)
}
