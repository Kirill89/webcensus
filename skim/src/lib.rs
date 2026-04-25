//! Streamy HTTPS prober.
//!
//! Sends a raw HTTP/1.1 request over TLS and closes the connection as soon
//! as the response status line has been parsed, avoiding most of the body
//! bandwidth. Designed for scanning many hosts where the only signal you
//! want is the HTTP status code (or a connection failure), plus an
//! optional cert validation outcome captured without aborting bad-cert
//! handshakes.
//!
//! Entry point: [`probe`].

pub mod input;
pub mod probe;
pub mod tls;

pub use input::parse_dns_line;
pub use probe::{ProbeConfig, ProbeOutcome, Timeouts, probe};
pub use tls::{
    CertOutcome, CertSlot, NoVerifier, RecordingVerifier, insecure_client_config,
    install_crypto_provider, recording_client_config, webpki_verifier,
};
