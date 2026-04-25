use std::net::IpAddr;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::Duration;

use rustls::client::danger::ServerCertVerifier;
use rustls::pki_types::ServerName;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

use crate::tls::{CertOutcome, CertSlot, recording_client_config};

/// Per-phase probe deadlines. Each phase's clock starts fresh; the total
/// worst-case probe time is the sum of all three.
#[derive(Clone, Copy, Debug)]
pub struct Timeouts {
    pub connect: Duration,
    pub handshake: Duration,
    pub read: Duration,
}

/// Static configuration shared across many probes. The actual TLS
/// `ClientConfig` is built per probe so that cert validation outcomes can
/// be captured into a per-probe slot; only the inner verifier is shared.
#[derive(Clone)]
pub struct ProbeConfig {
    pub path: String,
    pub user_agent: String,
    pub port: u16,
    pub timeouts: Timeouts,
    pub inner_verifier: Arc<dyn ServerCertVerifier>,
}

/// Result of a single probe.
#[derive(Debug, Default)]
pub struct ProbeOutcome {
    /// HTTP status code if a status line was parsed; `None` on any failure.
    pub code: Option<u16>,
    /// Cert chain validation result, if the TLS handshake reached the
    /// `verify_server_cert` callback. `None` means the handshake failed
    /// before the server's certificate was checked.
    pub cert: Option<CertOutcome>,
}

/// Maximum bytes read while looking for the response's first `\r\n`. The
/// status line ("HTTP/1.1 200 OK\r\n") is always well under this; if the
/// cap is reached without a CRLF, the server is misbehaving and we bail.
const MAX_STATUS_PEEK: usize = 32;

/// Probe a single host.
///
/// Returns a [`ProbeOutcome`] containing the HTTP status code (if reached)
/// and cert chain validation outcome (if the handshake got that far). Any
/// failure — timeout per phase, connect error, TLS error, malformed
/// response — surfaces as `code: None`; the cert field is independent.
///
/// The connection is closed as soon as the status line is parsed.
pub async fn probe(host: &str, ip: IpAddr, cfg: &ProbeConfig) -> ProbeOutcome {
    let slot: CertSlot = Arc::new(StdMutex::new(None));
    let tls_cfg = recording_client_config(cfg.inner_verifier.clone(), slot.clone());

    let code = async {
        let stream = tokio::time::timeout(cfg.timeouts.connect, TcpStream::connect((ip, cfg.port)))
            .await
            .ok()?
            .ok()?;
        let _ = stream.set_nodelay(true);

        let server_name = ServerName::try_from(host.to_string()).ok()?;
        let mut tls = tokio::time::timeout(
            cfg.timeouts.handshake,
            TlsConnector::from(tls_cfg).connect(server_name, stream),
        )
        .await
        .ok()?
        .ok()?;

        let request = format!(
            "GET {path} HTTP/1.1\r\n\
             Host: {host}\r\n\
             User-Agent: {ua}\r\n\
             Accept: */*\r\n\
             Connection: close\r\n\
             \r\n",
            path = cfg.path,
            ua = cfg.user_agent,
        );

        tokio::time::timeout(cfg.timeouts.read, async move {
            tls.write_all(request.as_bytes()).await.ok()?;

            let mut buf = [0u8; MAX_STATUS_PEEK];
            let mut len = 0;
            while len < MAX_STATUS_PEEK {
                let n = tls.read(&mut buf[len..]).await.ok()?;
                if n == 0 {
                    break;
                }
                len += n;
                if buf[..len].windows(2).any(|w| w == b"\r\n") {
                    break;
                }
            }
            parse_status_code(&buf[..len])
        })
        .await
        .ok()
        .flatten()
    }
    .await;

    let cert = slot.lock().unwrap().take();
    ProbeOutcome { code, cert }
}

fn parse_status_code(buf: &[u8]) -> Option<u16> {
    let line = std::str::from_utf8(buf).ok()?.split("\r\n").next()?;
    let mut parts = line.split_whitespace();
    if !parts.next()?.starts_with("HTTP/") {
        return None;
    }
    parts.next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::parse_status_code;

    #[test]
    fn parses_well_formed_status_line() {
        assert_eq!(parse_status_code(b"HTTP/1.1 200 OK\r\n"), Some(200));
        assert_eq!(parse_status_code(b"HTTP/1.0 404 Not Found\r\n"), Some(404));
        assert_eq!(parse_status_code(b"HTTP/2 301\r\n"), Some(301));
    }

    #[test]
    fn rejects_non_http() {
        assert_eq!(parse_status_code(b"SSH-2.0-OpenSSH\r\n"), None);
        assert_eq!(parse_status_code(b""), None);
        assert_eq!(parse_status_code(b"HTTP/1.1\r\n"), None);
    }
}
