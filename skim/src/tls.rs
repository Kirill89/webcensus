//! TLS configuration helpers.

use std::sync::Arc;
use std::sync::Mutex as StdMutex;

use anyhow::{Context, Result};
use rustls::ClientConfig;
use rustls::DigitallySignedStruct;
use rustls::RootCertStore;
use rustls::SignatureScheme;
use rustls::client::WebPkiServerVerifier;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};

/// Outcome of a per-probe certificate chain validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CertOutcome {
    Valid,
    Invalid(String),
}

/// Slot a [`RecordingVerifier`] writes its cert validation outcome into.
/// One slot per probe.
pub type CertSlot = Arc<StdMutex<Option<CertOutcome>>>;

/// Certificate verifier that accepts every certificate without validation.
///
/// # Warning
///
/// This disables all TLS authenticity guarantees and exposes the connection
/// to MITM. Prefer [`RecordingVerifier`] when you want to know whether a
/// cert *would* have been valid.
#[derive(Debug)]
pub struct NoVerifier;

impl ServerCertVerifier for NoVerifier {
    fn verify_server_cert(
        &self,
        _: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &CertificateDer<'_>,
        _: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _: &[u8],
        _: &CertificateDer<'_>,
        _: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
        ]
    }
}

/// A verifier that performs full chain validation, records the outcome into
/// a per-probe [`CertSlot`], and **always returns `Ok`** so the handshake
/// proceeds.
///
/// This lets the probe complete (and yield an HTTP status code) even for
/// hosts with bad certs, while still capturing whether the cert would have
/// been trusted by the supplied inner verifier.
#[derive(Debug)]
pub struct RecordingVerifier {
    inner: Arc<dyn ServerCertVerifier>,
    slot: CertSlot,
}

impl RecordingVerifier {
    pub fn new(inner: Arc<dyn ServerCertVerifier>, slot: CertSlot) -> Arc<Self> {
        Arc::new(Self { inner, slot })
    }
}

impl ServerCertVerifier for RecordingVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let result = self.inner.verify_server_cert(
            end_entity,
            intermediates,
            server_name,
            ocsp_response,
            now,
        );
        let outcome = match &result {
            Ok(_) => CertOutcome::Valid,
            Err(e) => CertOutcome::Invalid(e.to_string()),
        };
        *self.slot.lock().unwrap() = Some(outcome);
        Ok(ServerCertVerified::assertion())
    }

    // We deliberately accept every handshake signature so even bogus-signature
    // sessions still yield a status code. Cert chain trust is captured via
    // `verify_server_cert` above; signature verification is a separate axis
    // we don't currently report.
    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &CertificateDer<'_>,
        _: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _: &[u8],
        _: &CertificateDer<'_>,
        _: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

/// Install rustls's `ring` provider as the process-wide default. Idempotent;
/// subsequent calls are a no-op.
pub fn install_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// Build a real chain-validating verifier backed by Mozilla's webpki roots.
/// Use it as the inner verifier passed to [`RecordingVerifier`].
pub fn webpki_verifier() -> Result<Arc<dyn ServerCertVerifier>> {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let verifier = WebPkiServerVerifier::builder(Arc::new(roots))
        .build()
        .context("building webpki verifier")?;
    Ok(verifier)
}

/// Build a [`ClientConfig`] that **does not validate server certificates**.
///
/// See [`NoVerifier`] for the safety implications.
pub fn insecure_client_config() -> Arc<ClientConfig> {
    let mut cfg = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoVerifier))
        .with_no_client_auth();
    cfg.alpn_protocols = vec![b"http/1.1".to_vec()];
    Arc::new(cfg)
}

/// Build a per-probe [`ClientConfig`] whose verifier records cert outcome
/// into `slot` and always accepts the handshake. The `inner_verifier` is
/// shared across all probes; only the slot is per-probe.
pub fn recording_client_config(
    inner_verifier: Arc<dyn ServerCertVerifier>,
    slot: CertSlot,
) -> Arc<ClientConfig> {
    let mut cfg = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(RecordingVerifier::new(inner_verifier, slot))
        .with_no_client_auth();
    cfg.alpn_protocols = vec![b"http/1.1".to_vec()];
    Arc::new(cfg)
}

#[cfg(test)]
mod tests {
    use super::{insecure_client_config, install_crypto_provider, webpki_verifier};

    #[test]
    fn config_advertises_http1_alpn() {
        install_crypto_provider();
        let cfg = insecure_client_config();
        assert_eq!(cfg.alpn_protocols, vec![b"http/1.1".to_vec()]);
    }

    #[test]
    fn install_is_idempotent() {
        install_crypto_provider();
        install_crypto_provider();
    }

    #[test]
    fn webpki_verifier_builds() {
        install_crypto_provider();
        webpki_verifier().expect("webpki verifier should build with bundled roots");
    }
}
