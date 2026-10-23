// SPDX-License-Identifier: Apache-2.0

//! TLS configuration for the reqwest 0.11 client required by `tendermint-rpc`.
//!
//! reqwest 0.11 is tied to rustls 0.21 / rustls-webpki 0.101, which no longer receive
//! certificate distrust updates. The verifier here keeps reqwest on rustls 0.21 for the
//! handshake but does all certificate parsing and verification with rustls 0.23 /
//! rustls-webpki 0.103, so the Tendermint RPC client follows the same distrust list as the
//! workspace's rustls 0.23 clients.

use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use rustls::{
    client::{danger::ServerCertVerifier as _, WebPkiServerVerifier},
    crypto::WebPkiSupportedAlgorithms,
    pki_types::{CertificateDer, ServerName, UnixTime},
    RootCertStore, SignatureScheme,
};
use rustls_0_21::client::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};

/// Signature schemes allowed in TLS 1.3 handshakes (RFC 8446, section 4.2.3).
const TLS13_SCHEMES: &[SignatureScheme] = &[
    SignatureScheme::ECDSA_NISTP256_SHA256,
    SignatureScheme::ECDSA_NISTP384_SHA384,
    SignatureScheme::ECDSA_NISTP521_SHA512,
    SignatureScheme::RSA_PSS_SHA256,
    SignatureScheme::RSA_PSS_SHA384,
    SignatureScheme::RSA_PSS_SHA512,
    SignatureScheme::ED25519,
    SignatureScheme::ED448,
];

/// Builds a rustls 0.21 client config for `reqwest_0_11::ClientBuilder::use_preconfigured_tls`
/// that validates server certificates with rustls-webpki 0.103.
///
/// Trust anchors come from the OS store. The bundled Mozilla roots (`webpki-roots`) are used
/// only when the OS store is absent, i.e. it yields no certificates and reports no errors.
///
/// # Errors
/// Returns an error if the OS store is configured but unreadable (for example a bad
/// `SSL_CERT_FILE`), or if the certificate verifier cannot be built.
pub fn client_config() -> anyhow::Result<rustls_0_21::ClientConfig> {
    let native = rustls_native_certs::load_native_certs();
    let mut roots = RootCertStore::empty();
    roots.add_parsable_certificates(native.certs);
    if roots.is_empty() {
        if !native.errors.is_empty() {
            let errors: Vec<_> = native.errors.iter().map(ToString::to_string).collect();
            anyhow::bail!(
                "failed to load native root certificates: {}",
                errors.join("; ")
            );
        }
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    }

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let algorithms = provider.signature_verification_algorithms;
    let inner = WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider).build()?;

    let mut config = rustls_0_21::ClientConfig::builder()
        .with_safe_defaults()
        .with_custom_certificate_verifier(Arc::new(Verifier { inner, algorithms }))
        .with_no_client_auth();
    // reqwest only sets ALPN on configs it builds itself; keep its default of offering HTTP/2
    // with HTTP/1.1 as the fallback.
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(config)
}

/// Adapts a rustls 0.23 [`WebPkiServerVerifier`] to the rustls 0.21 verifier trait.
struct Verifier {
    inner: Arc<WebPkiServerVerifier>,
    algorithms: WebPkiSupportedAlgorithms,
}

impl Verifier {
    /// Verifies a handshake signature with rustls-webpki 0.103 instead of the rustls 0.21
    /// default, which would re-parse the end-entity certificate with rustls-webpki 0.101.
    fn verify_signature(
        &self,
        message: &[u8],
        cert: &rustls_0_21::Certificate,
        dss: &rustls_0_21::DigitallySignedStruct,
        tls13: bool,
    ) -> Result<HandshakeSignatureValid, rustls_0_21::Error> {
        let unadvertised = || {
            rustls_0_21::Error::PeerMisbehaved(
                rustls_0_21::PeerMisbehaved::SignedHandshakeWithUnadvertisedSigScheme,
            )
        };

        let scheme = SignatureScheme::from(dss.scheme.get_u16());
        if tls13 && !TLS13_SCHEMES.contains(&scheme) {
            return Err(unadvertised());
        }
        let algs = self
            .algorithms
            .mapping
            .iter()
            .find(|(s, _)| *s == scheme)
            .map(|(_, algs)| *algs)
            .ok_or_else(unadvertised)?;
        // TLS 1.3 binds a scheme to a single algorithm; TLS 1.2 may need to try several.
        let algs = if tls13 {
            algs.get(..1).unwrap_or_default()
        } else {
            algs
        };

        let cert = CertificateDer::from(cert.0.as_slice());
        let cert = webpki::EndEntityCert::try_from(&cert).map_err(invalid_certificate)?;
        let mut error = None;
        for alg in algs {
            match cert.verify_signature(*alg, message, dss.signature()) {
                Ok(()) => return Ok(HandshakeSignatureValid::assertion()),
                Err(e) => error = Some(e),
            }
        }
        Err(error.map_or_else(unadvertised, invalid_certificate))
    }
}

impl ServerCertVerifier for Verifier {
    fn verify_server_cert(
        &self,
        end_entity: &rustls_0_21::Certificate,
        intermediates: &[rustls_0_21::Certificate],
        server_name: &rustls_0_21::ServerName,
        _scts: &mut dyn Iterator<Item = &[u8]>,
        ocsp_response: &[u8],
        now: SystemTime,
    ) -> Result<ServerCertVerified, rustls_0_21::Error> {
        let server_name = match server_name {
            rustls_0_21::ServerName::DnsName(name) => ServerName::try_from(name.as_ref())
                .map_err(|e| rustls_0_21::Error::General(e.to_string()))?,
            rustls_0_21::ServerName::IpAddress(ip) => ServerName::IpAddress((*ip).into()),
            _ => {
                return Err(rustls_0_21::Error::General(
                    "unsupported server name type".to_string(),
                ))
            }
        };
        let now = now
            .duration_since(UNIX_EPOCH)
            .map_err(|e| rustls_0_21::Error::General(e.to_string()))?;
        let intermediates: Vec<_> = intermediates
            .iter()
            .map(|cert| CertificateDer::from(cert.0.as_slice()))
            .collect();

        self.inner
            .verify_server_cert(
                &CertificateDer::from(end_entity.0.as_slice()),
                &intermediates,
                &server_name,
                ocsp_response,
                UnixTime::since_unix_epoch(now),
            )
            .map(|_| ServerCertVerified::assertion())
            .map_err(|e| match e {
                rustls::Error::InvalidCertificate(_) => rustls_0_21::Error::InvalidCertificate(
                    rustls_0_21::CertificateError::Other(Arc::new(e)),
                ),
                e => rustls_0_21::Error::General(e.to_string()),
            })
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls_0_21::Certificate,
        dss: &rustls_0_21::DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls_0_21::Error> {
        self.verify_signature(message, cert, dss, false)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls_0_21::Certificate,
        dss: &rustls_0_21::DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls_0_21::Error> {
        self.verify_signature(message, cert, dss, true)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls_0_21::SignatureScheme> {
        self.algorithms
            .supported_schemes()
            .into_iter()
            .map(|scheme| rustls_0_21::SignatureScheme::from(u16::from(scheme)))
            .collect()
    }
}

fn invalid_certificate(error: webpki::Error) -> rustls_0_21::Error {
    let error = match error {
        webpki::Error::InvalidSignatureForPublicKey => rustls_0_21::CertificateError::BadSignature,
        error => rustls_0_21::CertificateError::Other(Arc::new(error)),
    };
    rustls_0_21::Error::InvalidCertificate(error)
}
