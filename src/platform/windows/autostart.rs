use anyhow::{bail, Result};
use std::os::windows::process::CommandExt;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x08000000;

fn run_reg_command(args: &[&str]) -> Result<()> {
    let mut cmd = Command::new("reg");
    cmd.args(args);
    cmd.creation_flags(CREATE_NO_WINDOW);
    let status = cmd.status()?;
    if !status.success() {
        bail!("reg command failed with status: {}", status);
    }
    Ok(())
}

pub fn enable_autostart() -> Result<()> {
    let app_path = std::env::current_exe()?;
    let val = format!("\"{}\" --minimized", app_path.display());
    run_reg_command(&[
        "add",
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
        "/v",
        "Outfox",
        "/t",
        "REG_SZ",
        "/d",
        &val,
        "/f",
    ])
}

pub fn disable_autostart() -> Result<()> {
    run_reg_command(&[
        "delete",
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
        "/v",
        "Outfox",
        "/f",
    ])
}

pub fn is_autostart_enabled() -> bool {
    let mut cmd = Command::new("reg");
    cmd.args(&[
        "query",
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
        "/v",
        "Outfox",
    ]);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd.status().map(|s| s.success()).unwrap_or(false)
}
