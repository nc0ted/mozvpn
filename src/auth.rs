use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use rand::RngCore;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::net::TcpStream;
use tokio::time::timeout;
use url::Url;

type HmacSha256 = Hmac<Sha256>;

const FASTLY_ANYCAST_IPS: &[Ipv4Addr] = &[
    Ipv4Addr::new(151, 101, 65, 91),
    Ipv4Addr::new(151, 101, 129, 91),
    Ipv4Addr::new(151, 101, 193, 91),
];

const FXA_CLIENT_ID: &str = "5882386c6d801776";
const FXA_AUTH_HOST: &str = "oauth.accounts.firefox.com";
const VPN_SCOPE: &str = "profile https://identity.mozilla.com/apps/vpn";
const GUARDIAN_HOST: &str = "vpn.mozilla.org";
const SALT_NAMESPACE: &[u8] = b"identity.mozilla.com/picl/v1/sessionToken";

use zeroize::Zeroizing;

#[derive(Debug, Clone, Default)]
pub struct QuotaInfo {
    pub unlimited: bool,
    pub max_bytes: u64,
    pub remaining_bytes: u64,
}

impl QuotaInfo {
    pub fn format_display(&self) -> String {
        if self.unlimited {
            "Unlimited bandwidth".to_string()
        } else {
            let rem_gb = (self.remaining_bytes as f64 / 1_073_741_824.0).round() as u64;
            let max_gb = (self.max_bytes as f64 / 1_073_741_824.0).round() as u64;
            if rem_gb == 0 && self.remaining_bytes > 0 {
                let rem_mb = (self.remaining_bytes as f64 / 1_048_576.0).round() as u64;
                format!("{} MB of {} GB left this month", rem_mb, max_gb)
            } else {
                format!("{} GB of {} GB left this month", rem_gb, max_gb)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct GuardianPass {
    pub jwt: Zeroizing<String>,
    pub exp: u64,
    pub quota: Option<QuotaInfo>,
}

impl GuardianPass {
    pub fn seconds_left(&self) -> i64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        self.exp as i64 - now
    }
}

pub async fn find_working_fastly_ip() -> Result<IpAddr> {
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    for &ip in FASTLY_ANYCAST_IPS {
        let tx = tx.clone();
        tokio::spawn(async move {
            let addr = SocketAddr::new(IpAddr::V4(ip), 443);
            if let Ok(Ok(_)) = timeout(Duration::from_millis(2000), TcpStream::connect(addr)).await {
                let _ = tx.send(ip).await;
            }
        });
    }
    drop(tx);
    if let Some(ip) = rx.recv().await {
        crate::log_info!("Selected alive Fastly Anycast POP: {}", ip);
        return Ok(IpAddr::V4(ip));
    }
    crate::log_error!("All Fastly anycast IPs failed to connect");
    bail!("no working Fastly anycast IP found")
}

pub async fn create_resilient_http_client() -> Result<Client> {
    if let Ok(working_ip) = find_working_fastly_ip().await {
        let fxa_addr = SocketAddr::new(working_ip, 443);
        let guardian_addr = SocketAddr::new(working_ip, 443);

        let client = Client::builder()
            .resolve(FXA_AUTH_HOST, fxa_addr)
            .resolve(GUARDIAN_HOST, guardian_addr)
            .timeout(Duration::from_secs(8))
            .build()?;
        return Ok(client);
    }

    crate::log_warn!("Using standard system DNS resolution fallback");
    let client = Client::builder()
        .timeout(Duration::from_secs(8))
        .build()?;
    Ok(client)
}

struct HawkKeys {
    id: String,
    auth_key: Vec<u8>,
}

fn derive_hawk_keys(session_token_hex: &str) -> Result<HawkKeys> {
    let token_bytes = hex::decode(session_token_hex.trim())
        .context("invalid hex session token")?;
    let hk = Hkdf::<Sha256>::new(Some(&[]), &token_bytes);
    let mut okm = [0u8; 96];
    hk.expand(SALT_NAMESPACE, &mut okm)
        .map_err(|_| anyhow::anyhow!("HKDF expansion failed"))?;

    let id = hex::encode(&okm[0..32]);
    let auth_key = okm[32..64].to_vec();
    Ok(HawkKeys { id, auth_key })
}

fn build_hawk_header(
    keys: &HawkKeys,
    method: &str,
    path: &str,
    host: &str,
    port: u16,
    body: &str,
) -> Result<String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_secs();
    let ts = now.to_string();

    let mut nonce_bytes = [0u8; 5];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = URL_SAFE_NO_PAD.encode(nonce_bytes);

    let mut payload_hasher = Sha256::new();
    payload_hasher.update(b"hawk.1.payload\napplication/json\n");
    payload_hasher.update(body.as_bytes());
    payload_hasher.update(b"\n");
    let payload_hash = STANDARD.encode(payload_hasher.finalize());

    let normalized = format!(
        "hawk.1.header\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n\n",
        ts,
        nonce,
        method.to_uppercase(),
        path,
        host.to_lowercase(),
        port,
        payload_hash
    );

    let mut mac = HmacSha256::new_from_slice(&keys.auth_key)
        .map_err(|_| anyhow::anyhow!("failed to create HMAC"))?;
    mac.update(normalized.as_bytes());
    let signature = STANDARD.encode(mac.finalize().into_bytes());

    Ok(format!(
        "Hawk id=\"{}\", ts=\"{}\", nonce=\"{}\", hash=\"{}\", mac=\"{}\"",
        keys.id, ts, nonce, payload_hash, signature
    ))
}

fn generate_pkce() -> (String, String) {
    let mut verifier_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut verifier_bytes);
    let verifier = URL_SAFE_NO_PAD.encode(verifier_bytes);

    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let challenge = URL_SAFE_NO_PAD.encode(hasher.finalize());

    (verifier, challenge)
}

#[derive(Deserialize)]
struct AuthCodeResponse {
    redirect: String,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(Deserialize)]
struct GuardianTokenResponse {
    token: String,
}

#[derive(Deserialize)]
struct JwtClaims {
    exp: u64,
}

async fn mint_pass_attempt(session_token_hex: &str) -> Result<GuardianPass> {
    let client = create_resilient_http_client().await?;
    let hawk_keys = derive_hawk_keys(session_token_hex)?;

    let mut state_bytes = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut state_bytes);
    let state = URL_SAFE_NO_PAD.encode(state_bytes);

    let (code_verifier, code_challenge) = generate_pkce();

    let auth_body = json!({
        "client_id": FXA_CLIENT_ID,
        "state": state,
        "scope": VPN_SCOPE,
        "code_challenge": code_challenge,
        "code_challenge_method": "S256"
    })
    .to_string();

    let auth_path = "/v1/oauth/authorization";
    let hawk_header = build_hawk_header(
        &hawk_keys,
        "POST",
        auth_path,
        FXA_AUTH_HOST,
        443,
        &auth_body,
    )?;

    let auth_url = format!("https://{}{}", FXA_AUTH_HOST, auth_path);
    let res = client
        .post(&auth_url)
        .header("Authorization", hawk_header)
        .header("Content-Type", "application/json")
        .body(auth_body)
        .send()
        .await
        .context("FxA authorization request failed")?;

    if !res.status().is_success() {
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        bail!("FxA authorization returned {}: {}", status, text);
    }

    let auth_data: AuthCodeResponse = res.json().await?;
    let parsed_url = Url::parse(&auth_data.redirect)
        .context("invalid redirect URL from FxA")?;

    let mut auth_code = None;
    for (k, v) in parsed_url.query_pairs() {
        if k == "code" {
            auth_code = Some(v.to_string());
            break;
        }
    }
    let auth_code = auth_code.context("code parameter not found in redirect URL")?;

    let trade_url = format!("https://{}/v1/token", FXA_AUTH_HOST);
    let trade_body = json!({
        "code": auth_code,
        "client_id": FXA_CLIENT_ID,
        "code_verifier": code_verifier
    });

    let trade_res = client
        .post(&trade_url)
        .json(&trade_body)
        .send()
        .await
        .context("FxA trade code request failed")?;

    if !trade_res.status().is_success() {
        let status = trade_res.status();
        let text = trade_res.text().await.unwrap_or_default();
        bail!("FxA trade code returned {}: {}", status, text);
    }

    let token_data: TokenResponse = trade_res.json().await?;

    let guardian_url = format!("https://{}/api/v1/fpn/token", GUARDIAN_HOST);
    let guardian_res = client
        .get(&guardian_url)
        .header("Authorization", format!("Bearer {}", token_data.access_token))
        .header("Content-Type", "application/json")
        .send()
        .await
        .context("Guardian token request failed")?;

    if !guardian_res.status().is_success() {
        let status = guardian_res.status();
        let text = guardian_res.text().await.unwrap_or_default();
        bail!("Guardian API returned {}: {}", status, text);
    }

    let headers = guardian_res.headers();
    let unlimited = headers
        .get("x-quota-unlimited")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let quota = if unlimited {
        Some(QuotaInfo {
            unlimited: true,
            max_bytes: 0,
            remaining_bytes: 0,
        })
    } else {
        let max_bytes = headers
            .get("x-quota-limit")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        let remaining_bytes = headers
            .get("x-quota-remaining")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);

        if max_bytes > 0 || remaining_bytes > 0 {
            Some(QuotaInfo {
                unlimited: false,
                max_bytes,
                remaining_bytes,
            })
        } else {
            None
        }
    };

    let guardian_data: GuardianTokenResponse = guardian_res.json().await?;
    let jwt = guardian_data.token;

    let parts: Vec<&str> = jwt.split('.').collect();
    if parts.len() < 2 {
        bail!("malformed JWT token received");
    }

    let payload_bytes = URL_SAFE_NO_PAD
        .decode(parts[1])
        .context("failed to decode JWT payload")?;
    let claims: JwtClaims = serde_json::from_slice(&payload_bytes)
        .context("failed to parse JWT claims")?;

    let pass = GuardianPass {
        jwt: Zeroizing::new(jwt),
        exp: claims.exp,
        quota,
    };
    crate::log_info!(
        "Guardian pass minted successfully, valid for {}s",
        pass.seconds_left()
    );
    Ok(pass)
}

pub async fn mint_pass(session_token_hex: &str) -> Result<GuardianPass> {
    match mint_pass_attempt(session_token_hex).await {
        Ok(pass) => Ok(pass),
        Err(e) => {
            crate::log_warn!("Initial pass mint failed: {:#}. Retrying...", e);
            mint_pass_attempt(session_token_hex).await
        }
    }
}
