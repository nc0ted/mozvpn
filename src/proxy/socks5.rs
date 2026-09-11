use crate::proxy::upstream::UpstreamClient;
use anyhow::{bail, Result};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use zeroize::Zeroizing;

pub async fn handle_socks5_connection(
    mut client_stream: TcpStream,
    upstream: Arc<UpstreamClient>,
    exit_host: Arc<String>,
    exit_port: u16,
    jwt_token: Arc<Zeroizing<String>>,
    mut reset_rx: tokio::sync::broadcast::Receiver<()>,
) -> Result<()> {
    let mut header = [0u8; 2];
    client_stream.read_exact(&mut header).await?;
    if header[0] != 0x05 {
        bail!("unsupported SOCKS version: {}", header[0]);
    }

    let num_methods = header[1] as usize;
    let mut methods = vec![0u8; num_methods];
    client_stream.read_exact(&mut methods).await?;

    if !methods.contains(&0x00) {
        client_stream.write_all(&[0x05, 0xff]).await?;
        bail!("no acceptable authentication methods");
    }

    client_stream.write_all(&[0x05, 0x00]).await?;
    client_stream.flush().await?;

    let mut req_header = [0u8; 4];
    client_stream.read_exact(&mut req_header).await?;

    if req_header[0] != 0x05 {
        bail!("invalid SOCKS5 request version");
    }
    if req_header[1] != 0x01 {
        client_stream
            .write_all(&[0x05, 0x07, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
            .await?;
        bail!("unsupported SOCKS5 command: {}", req_header[1]);
    }

    let target_host = match req_header[3] {
        0x01 => {
            let mut ip_bytes = [0u8; 4];
            client_stream.read_exact(&mut ip_bytes).await?;
            Ipv4Addr::from(ip_bytes).to_string()
        }
        0x03 => {
            let len = client_stream.read_u8().await? as usize;
            let mut domain_bytes = vec![0u8; len];
            client_stream.read_exact(&mut domain_bytes).await?;
            String::from_utf8(domain_bytes)?
        }
        0x04 => {
            let mut ip_bytes = [0u8; 16];
            client_stream.read_exact(&mut ip_bytes).await?;
            Ipv6Addr::from(ip_bytes).to_string()
        }
        atyp => {
            client_stream
                .write_all(&[0x05, 0x08, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                .await?;
            bail!("unsupported SOCKS5 address type: {}", atyp);
        }
    };

    let target_port = client_stream.read_u16().await?;
    crate::log_info!("SOCKS5 CONNECT {}:{}", target_host, target_port);

    let mut upstream_stream = match upstream
        .connect(&exit_host, exit_port, &target_host, target_port, &jwt_token)
        .await
    {
        Ok(s) => s,
        Err(err) => {
            let _ = client_stream
                .write_all(&[0x05, 0x05, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                .await;
            return Err(err);
        }
    };

    client_stream
        .write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
        .await?;
    tokio::select! {
        res = tokio::io::copy_bidirectional(&mut client_stream, &mut upstream_stream) => {
            res?;
        }
        _ = reset_rx.recv() => {}
    }
    Ok(())
}
