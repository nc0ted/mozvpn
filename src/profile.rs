use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Deserialize, Debug, Clone)]
pub struct AccountData {
    pub email: Option<String>,
    #[serde(rename = "sessionToken")]
    pub session_token: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct SignedInUser {
    #[serde(rename = "accountData")]
    pub account_data: AccountData,
}

pub fn find_firefox_profile_file() -> Result<PathBuf> {
    let base_dirs = candidate_profile_directories();
    for base in base_dirs {
        if !base.exists() {
            continue;
        }
        if let Ok(entries) = fs::read_dir(&base) {
            for entry in entries.flatten() {
                let candidate = entry.path().join("signedInUser.json");
                if candidate.exists() && candidate.is_file() {
                    crate::log_info!("Found Firefox profile: {}", candidate.display());
                    return Ok(candidate);
                }
            }
        }
    }
    crate::log_warn!("No Firefox profile found in standard paths");
    anyhow::bail!("signedInUser.json not found in standard Firefox profiles")
}

pub fn load_session_token_from_file(path: &Path) -> Result<AccountData> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read file: {}", path.display()))?;
    let parsed: SignedInUser = serde_json::from_str(&content)
        .with_context(|| "failed to parse signedInUser.json")?;
    crate::log_info!("Loaded profile account: {:?}", parsed.account_data.email);
    Ok(parsed.account_data)
}

pub fn auto_load_account_data() -> Result<AccountData> {
    let profile_path = find_firefox_profile_file()?;
    load_session_token_from_file(&profile_path)
}

fn candidate_profile_directories() -> Vec<PathBuf> {
    let mut dirs_list = Vec::new();
    if let Some(home) = dirs::home_dir() {
        dirs_list.push(home.join(".mozilla").join("firefox"));
        dirs_list.push(home.join(".var").join("app").join("org.mozilla.firefox").join(".mozilla").join("firefox"));
        dirs_list.push(home.join("Library").join("Application Support").join("Firefox").join("Profiles"));
    }
    if let Some(data) = dirs::data_dir() {
        dirs_list.push(data.join("Mozilla").join("Firefox").join("Profiles"));
    }
    dirs_list
}
