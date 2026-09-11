use anyhow::{bail, Result};
use std::process::Command;

fn get_active_macos_network_service() -> Option<String> {
    let route_output = Command::new("route").args(["-n", "get", "default"]).output().ok()?;
    let route_text = String::from_utf8_lossy(&route_output.stdout);
    let dev = route_text
        .lines()
        .find(|l| l.trim().starts_with("interface:"))?
        .split(':')
        .nth(1)?
        .trim();

    let order_output = Command::new("networksetup").arg("-listnetworkserviceorder").output().ok()?;
    let order_text = String::from_utf8_lossy(&order_output.stdout);
    let target = format!("Device: {})", dev);

    let lines: Vec<&str> = order_text.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if line.contains(&target) && i > 0 {
            let prev = lines[i - 1];
            if let Some(start) = prev.find(')') {
                let service = prev[start + 1..].trim();
                if !service.is_empty() {
                    return Some(service.to_string());
                }
            }
        }
    }
    None
}

fn get_macos_network_services() -> Vec<String> {
    if let Some(active) = get_active_macos_network_service() {
        return vec![active];
    }
    if let Ok(output) = Command::new("networksetup").arg("-listallnetworkservices").output() {
        let text = String::from_utf8_lossy(&output.stdout);
        let services: Vec<String> = text
            .lines()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty() && !s.starts_with('*') && !s.starts_with("An asterisk"))
            .map(|s| s.to_string())
            .collect();
        if !services.is_empty() {
            return services;
        }
    }
    vec!["Wi-Fi".to_string()]
}

pub struct SystemProxy;

impl SystemProxy {
    pub fn enable(http_port: u16, socks_port: u16) -> Result<()> {
        crate::log_info!("Setting OS system proxy to HTTP :{}, SOCKS5 :{}", http_port, socks_port);
        let services = get_macos_network_services();
        let mut any_success = false;
        for service in &services {
            let s1 = Command::new("networksetup")
                .args(["-setwebproxy", service, "127.0.0.1", &http_port.to_string()])
                .status();
            let s2 = Command::new("networksetup")
                .args(["-setsecurewebproxy", service, "127.0.0.1", &http_port.to_string()])
                .status();
            let s3 = Command::new("networksetup")
                .args(["-setsocksfirewallproxy", service, "127.0.0.1", &socks_port.to_string()])
                .status();
            if let (Ok(r1), Ok(r2), Ok(r3)) = (s1, s2, s3) {
                if r1.success() && r2.success() && r3.success() {
                    any_success = true;
                }
            }
        }
        if !any_success {
            bail!("networksetup failed to enable proxy for services: {:?}", services);
        }
        Ok(())
    }

    pub fn disable() -> Result<()> {
        crate::log_info!("Disabling OS system proxy");
        let services = get_macos_network_services();
        let mut any_success = false;
        for service in &services {
            let s1 = Command::new("networksetup")
                .args(["-setwebproxystate", service, "off"])
                .status();
            let s2 = Command::new("networksetup")
                .args(["-setsecurewebproxystate", service, "off"])
                .status();
            let s3 = Command::new("networksetup")
                .args(["-setsocksfirewallproxystate", service, "off"])
                .status();
            if let (Ok(r1), Ok(r2), Ok(r3)) = (s1, s2, s3) {
                if r1.success() && r2.success() && r3.success() {
                    any_success = true;
                }
            }
        }
        if !any_success {
            bail!("networksetup failed to disable proxy for services: {:?}", services);
        }
        Ok(())
    }
}
