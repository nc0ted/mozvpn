use anyhow::{bail, Result};
use std::os::windows::process::CommandExt;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x08000000;

#[link(name = "wininet")]
extern "system" {
    fn InternetSetOptionW(
        hinternet: *mut std::ffi::c_void,
        dwoption: u32,
        lpbuffer: *const std::ffi::c_void,
        dwbufferlength: u32,
    ) -> i32;
}

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

fn notify_wininet() {
    const INTERNET_OPTION_SETTINGS_CHANGED: u32 = 39;
    const INTERNET_OPTION_REFRESH: u32 = 37;
    unsafe {
        InternetSetOptionW(std::ptr::null_mut(), INTERNET_OPTION_SETTINGS_CHANGED, std::ptr::null(), 0);
        InternetSetOptionW(std::ptr::null_mut(), INTERNET_OPTION_REFRESH, std::ptr::null(), 0);
    }
}

pub struct SystemProxy;

impl SystemProxy {
    pub fn enable(http_port: u16, socks_port: u16) -> Result<()> {
        crate::log_info!("Setting OS system proxy to HTTP :{}, SOCKS5 :{}", http_port, socks_port);
        let proxy_val = format!(
            "http=127.0.0.1:{};https=127.0.0.1:{};socks=127.0.0.1:{}",
            http_port, http_port, socks_port
        );
        run_reg_command(&[
            "add",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
            "/v",
            "ProxyEnable",
            "/t",
            "REG_DWORD",
            "/d",
            "1",
            "/f",
        ])?;
        run_reg_command(&[
            "add",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
            "/v",
            "ProxyServer",
            "/t",
            "REG_SZ",
            "/d",
            &proxy_val,
            "/f",
        ])?;
        notify_wininet();
        Ok(())
    }

    pub fn disable() -> Result<()> {
        crate::log_info!("Disabling OS system proxy");
        run_reg_command(&[
            "add",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
            "/v",
            "ProxyEnable",
            "/t",
            "REG_DWORD",
            "/d",
            "0",
            "/f",
        ])?;
        notify_wininet();
        Ok(())
    }
}
