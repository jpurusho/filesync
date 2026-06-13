use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{DigitallySignedStruct, DistinguishedName, SignatureScheme};
use tokio_rustls::{TlsAcceptor, TlsConnector};

use crate::Error;
use crate::identity::{Fingerprint, Identity};

/// Build a TLS acceptor (server side) that requires mutual TLS.
/// During pairing, accepts any client cert. After pairing, only pinned certs.
pub fn make_server_config(
    identity: &Identity,
    pinned_certs: &[CertificateDer<'static>],
    pairing_mode: bool,
) -> Result<Arc<rustls::ServerConfig>, Error> {
    let verifier: Arc<dyn ClientCertVerifier> = if pairing_mode {
        Arc::new(AcceptAnyClientCert)
    } else {
        Arc::new(PinnedClientVerifier::new(pinned_certs))
    };

    let config = rustls::ServerConfig::builder()
        .with_client_cert_verifier(verifier)
        .with_single_cert(identity.rustls_cert_chain(), identity.rustls_private_key())
        .map_err(|e| Error::Tls(format!("server config: {e}")))?;

    Ok(Arc::new(config))
}

/// Build a TLS connector (client side).
/// During pairing, accepts any server cert. After pairing, only pinned certs.
pub fn make_client_config(
    identity: &Identity,
    pinned_certs: &[CertificateDer<'static>],
    pairing_mode: bool,
) -> Result<Arc<rustls::ClientConfig>, Error> {
    let verifier: Arc<dyn ServerCertVerifier> = if pairing_mode {
        Arc::new(AcceptAnyServerCert)
    } else {
        Arc::new(PinnedServerVerifier::new(pinned_certs))
    };

    let config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_client_auth_cert(identity.rustls_cert_chain(), identity.rustls_private_key())
        .map_err(|e| Error::Tls(format!("client config: {e}")))?;

    Ok(Arc::new(config))
}

pub fn make_acceptor(config: Arc<rustls::ServerConfig>) -> TlsAcceptor {
    TlsAcceptor::from(config)
}

pub fn make_connector(config: Arc<rustls::ClientConfig>) -> TlsConnector {
    TlsConnector::from(config)
}

// --- Accept-any verifiers (for pairing) ---

#[derive(Debug)]
struct AcceptAnyClientCert;

impl ClientCertVerifier for AcceptAnyClientCert {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::ED25519,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::RSA_PSS_SHA256,
        ]
    }

    fn client_auth_mandatory(&self) -> bool {
        false
    }
}

#[derive(Debug)]
struct AcceptAnyServerCert;

impl ServerCertVerifier for AcceptAnyServerCert {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::ED25519,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::RSA_PSS_SHA256,
        ]
    }
}

// --- Pinned cert verifiers (post-pairing) ---

#[derive(Debug)]
struct PinnedClientVerifier {
    fingerprints: Vec<Fingerprint>,
}

impl PinnedClientVerifier {
    fn new(certs: &[CertificateDer<'static>]) -> Self {
        let fingerprints = certs
            .iter()
            .map(|c| Fingerprint::from_cert_der(c.as_ref()))
            .collect();
        Self { fingerprints }
    }
}

impl ClientCertVerifier for PinnedClientVerifier {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        let fp = Fingerprint::from_cert_der(end_entity.as_ref());
        if self.fingerprints.contains(&fp) {
            Ok(ClientCertVerified::assertion())
        } else {
            Err(rustls::Error::General(
                "client certificate not in pinned set".to_owned(),
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::ED25519,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::RSA_PSS_SHA256,
        ]
    }

    fn client_auth_mandatory(&self) -> bool {
        true
    }
}

#[derive(Debug)]
struct PinnedServerVerifier {
    fingerprints: Vec<Fingerprint>,
}

impl PinnedServerVerifier {
    fn new(certs: &[CertificateDer<'static>]) -> Self {
        let fingerprints = certs
            .iter()
            .map(|c| Fingerprint::from_cert_der(c.as_ref()))
            .collect();
        Self { fingerprints }
    }
}

impl ServerCertVerifier for PinnedServerVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let fp = Fingerprint::from_cert_der(end_entity.as_ref());
        if self.fingerprints.contains(&fp) {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General(
                "server certificate not in pinned set".to_owned(),
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::ED25519,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::RSA_PSS_SHA256,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::Identity;

    #[test]
    fn server_config_pairing_mode() {
        let id = Identity::generate().unwrap();
        let config = make_server_config(&id, &[], true).unwrap();
        assert!(Arc::strong_count(&config) == 1);
    }

    #[test]
    fn client_config_pairing_mode() {
        let id = Identity::generate().unwrap();
        let config = make_client_config(&id, &[], true).unwrap();
        assert!(Arc::strong_count(&config) == 1);
    }
}
