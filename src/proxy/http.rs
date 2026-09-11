use crate::proxy::upstream::UpstreamClient;
use anyhow::{bail, Context, Result};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use url::Url;
use zeroize::Zeroizing;

pub async fn handle_http_connection(
    mut client_stream: TcpStream,
    upstream: Arc<UpstreamClient>,
    exit_host: Arc<String>,
    exit_port: u16,
    jwt_token: Arc<Zeroizing<String>>,
    mut reset_rx: tokio::sync::broadcast::Receiver<()>,
) -> Result<()> {
    let mut reader = BufReader::new(&mut client_stream);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).await?;

    if request_line.is_empty() {
        return Ok(());
    }

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 3 {
        bail!("malformed HTTP request line");
    }

    let method = parts[0];
    let target = parts[1];

    let mut headers = Vec::new();
    loop {
        let mut line = String::new();
        let bytes_read = reader.read_line(&mut line).await?;
        if bytes_read == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        headers.push(line);
    }

    if method.eq_ignore_ascii_case("CONNECT") {
        let (host, port) = parse_host_port(target, 443)?;
        crate::log_info!("HTTP CONNECT {}:{}", host, port);
        let mut upstream_stream = upstream
            .connect(&exit_host, exit_port, &host, port, &jwt_token)
            .await?;

        client_stream
            .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
            .await?;
        client_stream.flush().await?;

        tokio::select! {
            res = tokio::io::copy_bidirectional(&mut client_stream, &mut upstream_stream) => {
                res?;
            }
            _ = reset_rx.recv() => {}
        }
    } else {
        let parsed_url = Url::parse(target).context("failed to parse HTTP target URL")?;
        let host = parsed_url
            .host_str()
            .context("missing host in target URL")?;
        let port = parsed_url.port().unwrap_or(80);
        let path = parsed_url.path();
        let query = parsed_url
            .query()
            .map(|q| format!("?{}", q))
            .unwrap_or_default();

        let mut upstream_stream = upstream
            .connect(&exit_host, exit_port, host, port, &jwt_token)
            .await?;

        let mut forwarded = format!("{} {}{} HTTP/1.1\r\n", method, path, query);
        for header in headers {
            if !header.to_lowercase().starts_with("proxy-") {
                forwarded.push_str(&header);
            }
        }
        forwarded.push_str("\r\n");

        upstream_stream.write_all(forwarded.as_bytes()).await?;
        upstream_stream.flush().await?;

        tokio::select! {
            res = tokio::io::copy_bidirectional(&mut client_stream, &mut upstream_stream) => {
                res?;
            }
            _ = reset_rx.recv() => {}
        }
    }

    Ok(())
}

fn parse_host_port(target: &str, default_port: u16) -> Result<(String, u16)> {
    if let Some((h, p)) = target.split_once(':') {
        let port: u16 = p.parse().context("invalid port number")?;
        Ok((h.to_string(), port))
    } else {
        Ok((target.to_string(), default_port))
    }
}
