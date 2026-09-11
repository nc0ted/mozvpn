use anyhow::{bail, Context, Result};
use rustls::pki_types::ServerName;
use rustls::ClientConfig;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;

pub struct UpstreamClient {
    connector: TlsConnector,
}

impl UpstreamClient {
    pub fn new() -> Result<Self> {
        let mut root_store = rustls::RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()?
            .with_root_certificates(root_store)
            .with_no_client_auth();
        Ok(Self {
            connector: TlsConnector::from(Arc::new(config)),
        })
    }

    pub async fn connect(
        &self,
        exit_host: &str,
        exit_port: u16,
        target_host: &str,
        target_port: u16,
        jwt_token: &str,
    ) -> Result<TlsStream<TcpStream>> {
        let tcp_stream = TcpStream::connect((exit_host, exit_port))
            .await
            .with_context(|| format!("failed to connect to exit {}:{}", exit_host, exit_port))?;

        let server_name = ServerName::try_from(exit_host.to_string())
            .map_err(|_| anyhow::anyhow!("invalid exit server name"))?;

        let mut tls_stream = self
            .connector
            .connect(server_name, tcp_stream)
            .await
            .with_context(|| format!("TLS handshake failed with {}:{}", exit_host, exit_port))?;

        let connect_req = format!(
            "CONNECT {}:{} HTTP/1.1\r\nHost: {}:{}\r\nProxy-Authorization: Bearer {}\r\nProxy-Connection: keep-alive\r\n\r\n",
            target_host, target_port, target_host, target_port, jwt_token
        );

        tls_stream.write_all(connect_req.as_bytes()).await?;
        tls_stream.flush().await?;

        let mut reader = BufReader::new(&mut tls_stream);
        let mut status_line = String::new();
        reader.read_line(&mut status_line).await?;

        if !status_line.contains(" 200") {
            bail!(
                "upstream CONNECT {}:{} failed: {}",
                target_host,
                target_port,
                status_line.trim()
            );
        }

        loop {
            let mut header_line = String::new();
            let bytes_read = reader.read_line(&mut header_line).await?;
            if bytes_read == 0 || header_line == "\r\n" || header_line == "\n" {
                break;
            }
        }

        Ok(tls_stream)
    }
}
