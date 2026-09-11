use crate::auth::{mint_pass, GuardianPass};
use crate::locations::EXIT_PORT;
use crate::proxy::http::handle_http_connection;
use crate::proxy::socks5::handle_socks5_connection;
use crate::proxy::upstream::UpstreamClient;
use anyhow::{Context, Result};
use std::net::TcpListener as StdTcpListener;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, RwLock};
use zeroize::Zeroizing;

pub struct ProxyConfig {
    pub session_token: Zeroizing<String>,
    pub exit_host: String,
    pub requested_http_port: u16,
    pub requested_socks_port: u16,
}

pub struct RunningProxy {
    pub requested_http_port: u16,
    pub requested_socks_port: u16,
    pub http_port: u16,
    pub socks_port: u16,
    pub exit_host: Arc<RwLock<String>>,
    pub token_expiry_timestamp: Arc<AtomicU64>,
    pub is_running: Arc<AtomicBool>,
    pub current_pass: Arc<RwLock<Option<GuardianPass>>>,
    shutdown_tx: broadcast::Sender<()>,
    reset_tx: broadcast::Sender<()>,
}

impl RunningProxy {
    pub async fn start(config: ProxyConfig) -> Result<Self> {
        let http_port = find_free_port(config.requested_http_port)
            .context("failed to allocate free HTTP port")?;
        let socks_port = find_free_port(config.requested_socks_port)
            .context("failed to allocate free SOCKS5 port")?;

        let initial_pass = mint_pass(&config.session_token).await?;
        let token_expiry_timestamp = Arc::new(AtomicU64::new(initial_pass.exp));
        let current_pass = Arc::new(RwLock::new(Some(initial_pass)));
        let exit_host = Arc::new(RwLock::new(config.exit_host));
        let upstream = Arc::new(UpstreamClient::new()?);
        let is_running = Arc::new(AtomicBool::new(true));

        let (shutdown_tx, _) = broadcast::channel(4);
        let (reset_tx, _) = broadcast::channel(16);

        let http_listener = TcpListener::bind(("127.0.0.1", http_port)).await?;
        let socks_listener = TcpListener::bind(("127.0.0.1", socks_port)).await?;

        crate::log_info!(
            "Listeners active: HTTP on 127.0.0.1:{}, SOCKS5 on 127.0.0.1:{}, exit={}",
            http_port,
            socks_port,
            exit_host.read().await
        );

        let pass_clone = Arc::clone(&current_pass);
        let expiry_clone = Arc::clone(&token_expiry_timestamp);
        let session_token = config.session_token.clone();
        let running_clone = Arc::clone(&is_running);
        let mut shutdown_rx_refresher = shutdown_tx.subscribe();

        tokio::spawn(async move {
            let mut backoff = Duration::from_secs(5);
            while running_clone.load(Ordering::Relaxed) {
                let seconds_left = {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    expiry_clone.load(Ordering::Relaxed).saturating_sub(now)
                };

                let sleep_duration = if seconds_left > 120 {
                    Duration::from_secs(seconds_left - 120)
                } else {
                    backoff
                };

                tokio::select! {
                    _ = tokio::time::sleep(sleep_duration) => {
                        match mint_pass(&session_token).await {
                            Ok(new_pass) => {
                                expiry_clone.store(new_pass.exp, Ordering::Relaxed);
                                *pass_clone.write().await = Some(new_pass);
                                backoff = Duration::from_secs(5);
                            }
                            Err(e) => {
                                crate::log_error!("Token refresh failed: {:#}. Retrying in {:?}", e, backoff);
                                backoff = (backoff * 2).min(Duration::from_secs(60));
                            }
                        }
                    }
                    _ = shutdown_rx_refresher.recv() => {
                        break;
                    }
                }
            }
        });

        let pass_http = Arc::clone(&current_pass);
        let exit_http = Arc::clone(&exit_host);
        let upstream_http = Arc::clone(&upstream);
        let running_http = Arc::clone(&is_running);
        let mut shutdown_rx_http = shutdown_tx.subscribe();
        let reset_tx_http = reset_tx.clone();

        tokio::spawn(async move {
            while running_http.load(Ordering::Relaxed) {
                tokio::select! {
                    accept_res = http_listener.accept() => {
                        if let Ok((stream, _)) = accept_res {
                            let up = Arc::clone(&upstream_http);
                            let host = Arc::new(exit_http.read().await.clone());
                            let token = {
                                let guard = pass_http.read().await;
                                guard.as_ref().map(|p| p.jwt.clone()).unwrap_or_default()
                            };
                            let token_arc = Arc::new(token);
                            let reset_rx = reset_tx_http.subscribe();
                            tokio::spawn(async move {
                                if let Err(e) = handle_http_connection(stream, up, host, EXIT_PORT, token_arc, reset_rx).await {
                                    crate::log_warn!("HTTP connection error: {:#}", e);
                                }
                            });
                        }
                    }
                    _ = shutdown_rx_http.recv() => {
                        break;
                    }
                }
            }
        });

        let pass_socks = Arc::clone(&current_pass);
        let exit_socks = Arc::clone(&exit_host);
        let upstream_socks = Arc::clone(&upstream);
        let running_socks = Arc::clone(&is_running);
        let mut shutdown_rx_socks = shutdown_tx.subscribe();
        let reset_tx_socks = reset_tx.clone();

        tokio::spawn(async move {
            while running_socks.load(Ordering::Relaxed) {
                tokio::select! {
                    accept_res = socks_listener.accept() => {
                        if let Ok((stream, _)) = accept_res {
                            let up = Arc::clone(&upstream_socks);
                            let host = Arc::new(exit_socks.read().await.clone());
                            let token = {
                                let guard = pass_socks.read().await;
                                guard.as_ref().map(|p| p.jwt.clone()).unwrap_or_default()
                            };
                            let token_arc = Arc::new(token);
                            let reset_rx = reset_tx_socks.subscribe();
                            tokio::spawn(async move {
                                if let Err(e) = handle_socks5_connection(stream, up, host, EXIT_PORT, token_arc, reset_rx).await {
                                    crate::log_warn!("SOCKS5 connection error: {:#}", e);
                                }
                            });
                        }
                    }
                    _ = shutdown_rx_socks.recv() => {
                        break;
                    }
                }
            }
        });

        Ok(RunningProxy {
            requested_http_port: config.requested_http_port,
            requested_socks_port: config.requested_socks_port,
            http_port,
            socks_port,
            exit_host,
            token_expiry_timestamp,
            is_running,
            current_pass,
            shutdown_tx,
            reset_tx,
        })
    }

    pub fn change_exit(&self, rt: &tokio::runtime::Runtime, new_exit: String) {
        let exit_host = Arc::clone(&self.exit_host);
        let reset_tx = self.reset_tx.clone();
        rt.spawn(async move {
            crate::log_info!("Switching exit server to {}", new_exit);
            *exit_host.write().await = new_exit;
            let _ = reset_tx.send(());
        });
    }

    pub fn stop(&self) {
        crate::log_info!("Stopping proxy listeners");
        self.is_running.store(false, Ordering::Relaxed);
        let _ = self.shutdown_tx.send(());
        let _ = self.reset_tx.send(());
        if let Ok(mut guard) = self.current_pass.try_write() {
            *guard = None;
        }
    }
}

pub fn is_port_free(port: u16) -> bool {
    StdTcpListener::bind(("127.0.0.1", port)).is_ok()
}

pub fn find_free_port(start: u16) -> Option<u16> {
    (start..(start + 100)).find(|&port| is_port_free(port))
}
