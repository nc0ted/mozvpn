use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppConfig {
    #[serde(default = "default_location")]
    pub location_code: String,
    #[serde(default = "default_http_port")]
    pub http_port: u16,
    #[serde(default = "default_socks_port")]
    pub socks_port: u16,
    #[serde(default)]
    pub system_proxy: bool,
    #[serde(default)]
    pub auto_connect: bool,
    #[serde(default)]
    pub start_with_system: bool,
    #[serde(default)]
    pub start_minimized: bool,
    #[serde(default)]
    pub allow_lan: bool,
}

fn default_location() -> String {
    "us".to_string()
}

fn default_http_port() -> u16 {
    2085
}

fn default_socks_port() -> u16 {
    2081
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            location_code: default_location(),
            http_port: default_http_port(),
            socks_port: default_socks_port(),
            system_proxy: false,
            auto_connect: false,
            start_with_system: false,
            start_minimized: false,
            allow_lan: false,
        }
    }
}

pub fn get_config_path() -> PathBuf {
    let base = dirs::config_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("outfox").join("config.json")
}

pub fn load_config() -> AppConfig {
    let path = get_config_path();
    if let Ok(data) = fs::read_to_string(&path) {
        if let Ok(cfg) = serde_json::from_str(&data) {
            return cfg;
        }
    }
    AppConfig::default()
}

pub fn save_config(cfg: &AppConfig) {
    let path = get_config_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(data) = serde_json::to_string_pretty(cfg) {
        let _ = fs::write(path, data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let json = r#"{"location_code":"DE","http_port":2085,"socks_port":2081,"system_proxy":true}"#;
        let cfg: AppConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.location_code, "DE");
        assert_eq!(cfg.http_port, 2085);
        assert_eq!(cfg.socks_port, 2081);
        assert!(cfg.system_proxy);
        assert!(!cfg.auto_connect);
        assert!(!cfg.start_with_system);
        assert!(!cfg.start_minimized);
        assert!(!cfg.allow_lan);
    }

    #[test]
    fn test_config_full_serialization() {
        let cfg = AppConfig {
            location_code: "FR".to_string(),
            http_port: 3000,
            socks_port: 3001,
            system_proxy: false,
            auto_connect: true,
            start_with_system: true,
            start_minimized: true,
            allow_lan: true,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.location_code, "FR");
        assert!(parsed.auto_connect);
        assert!(parsed.start_with_system);
        assert!(parsed.start_minimized);
        assert!(parsed.allow_lan);
    }
}
