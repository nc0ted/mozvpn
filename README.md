# MozVPN

Exposes Firefox built-in VPN (Mozilla IP Protection / Guardian) to your entire operating system.

## Features

- **Direct Fastly MASQUE Tunneling**: Connects directly to Mozilla egress nodes over TLS 1.3 with automatic token refresh.
- **Auto-Discovery**: Detects active Firefox account and session tokens from local profiles (`signedInUser.json`).
- **Local Proxy Endpoints**: Exposes configurable local HTTP CONNECT and SOCKS5 proxy interfaces.
- **One-Click System Proxy**: Integrates with Linux (GNOME/gsettings), Windows Registry, and macOS network proxy settings.
- **Zero External Dependencies**: Standalone Rust + egui binary. No external runtime or browser extension required.

## Installation

Download prebuilt binaries for Linux, Windows, and macOS from [Releases](https://github.com/nc0ted/mozvpn/releases).

### Build from source (optional)

Requires Rust 1.80+:

```bash
cargo build --release
./target/release/mozvpn
```

## Default Ports

- **HTTP CONNECT**: `127.0.0.1:2085`
- **SOCKS5**: `127.0.0.1:2081`

## Disclaimer

Routing non-browser traffic through Mozilla's relay infrastructure may violate Mozilla's Terms of Service. Provided for educational and research purposes only. Use at your own risk.
