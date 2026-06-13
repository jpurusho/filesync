use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use rcgen::{CertificateParams, KeyPair, PKCS_ED25519};
use ring::rand::SystemRandom;
use ring::signature::Ed25519KeyPair;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::Error;

const KEY_FILENAME: &str = "identity.key";
const CERT_FILENAME: &str = "identity.cert";

/// A node's persistent identity: Ed25519 keypair + self-signed X.509 certificate.
#[derive(Clone)]
pub struct Identity {
    pub id: Uuid,
    pub cert_der: CertificateDer<'static>,
    pub cert_pem: String,
    key_pkcs8: Vec<u8>,
    pub fingerprint: Fingerprint,
}

/// SHA-256 fingerprint of the DER-encoded certificate.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Fingerprint {
    bytes: [u8; 32],
}

impl Fingerprint {
    pub fn from_cert_der(cert_der: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(cert_der);
        let result = hasher.finalize();
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&result);
        Self { bytes }
    }

    /// Short form for display and mDNS TXT: first 8 bytes as hex with colons.
    pub fn short(&self) -> String {
        self.bytes[..8]
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(":")
    }

    /// Full fingerprint as hex with colons.
    pub fn full(&self) -> String {
        self.bytes
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(":")
    }

    pub fn bytes(&self) -> &[u8; 32] {
        &self.bytes
    }
}

impl std::fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.short())
    }
}

impl Identity {
    /// Load identity from disk, or generate a new one if not present.
    pub fn load_or_generate(data_dir: &Path) -> Result<Self, Error> {
        let key_path = data_dir.join(KEY_FILENAME);
        let cert_path = data_dir.join(CERT_FILENAME);

        if key_path.exists() && cert_path.exists() {
            Self::load(&key_path, &cert_path)
        } else {
            let identity = Self::generate()?;
            identity.save(data_dir)?;
            Ok(identity)
        }
    }

    /// Generate a fresh Ed25519 identity.
    pub fn generate() -> Result<Self, Error> {
        let id = Uuid::new_v4();

        let rng = SystemRandom::new();
        let pkcs8_doc = Ed25519KeyPair::generate_pkcs8(&rng)
            .map_err(|e| Error::Identity(format!("key generation failed: {e}")))?;
        let pkcs8_bytes = pkcs8_doc.as_ref().to_vec();

        let pkcs8_key = PrivatePkcs8KeyDer::from(pkcs8_bytes.clone());
        let key_pair = KeyPair::from_pkcs8_der_and_sign_algo(&pkcs8_key, &PKCS_ED25519)
            .map_err(|e| Error::Identity(format!("key pair creation failed: {e}")))?;

        let mut params = CertificateParams::new(vec![id.to_string()])
            .map_err(|e| Error::Identity(format!("cert params failed: {e}")))?;
        params.distinguished_name.push(
            rcgen::DnType::CommonName,
            rcgen::DnValue::Utf8String(format!("FileSync Instance {id}")),
        );

        let cert = params
            .self_signed(&key_pair)
            .map_err(|e| Error::Identity(format!("self-signing failed: {e}")))?;

        let cert_pem = cert.pem();
        let cert_der = CertificateDer::from(cert.der().to_vec());
        let fingerprint = Fingerprint::from_cert_der(cert_der.as_ref());

        Ok(Self {
            id,
            cert_der,
            cert_pem,
            key_pkcs8: pkcs8_bytes,
            fingerprint,
        })
    }

    fn load(key_path: &Path, cert_path: &Path) -> Result<Self, Error> {
        let key_pem_str =
            fs::read_to_string(key_path).map_err(|e| Error::Identity(format!("read key: {e}")))?;
        let cert_pem_str = fs::read_to_string(cert_path)
            .map_err(|e| Error::Identity(format!("read cert: {e}")))?;

        let key_pem =
            pem::parse(&key_pem_str).map_err(|e| Error::Identity(format!("parse key PEM: {e}")))?;
        let cert_pem_parsed = pem::parse(&cert_pem_str)
            .map_err(|e| Error::Identity(format!("parse cert PEM: {e}")))?;

        let key_bytes = key_pem.contents().to_vec();
        let cert_der = CertificateDer::from(cert_pem_parsed.contents().to_vec());
        let fingerprint = Fingerprint::from_cert_der(cert_der.as_ref());

        // Derive a stable UUID from the fingerprint (since we can't easily extract SAN from DER)
        let id = Uuid::from_bytes(fingerprint.bytes[..16].try_into().unwrap());

        Ok(Self {
            id,
            cert_der,
            cert_pem: cert_pem_str,
            key_pkcs8: key_bytes,
            fingerprint,
        })
    }

    fn save(&self, data_dir: &Path) -> Result<(), Error> {
        fs::create_dir_all(data_dir)
            .map_err(|e| Error::Identity(format!("create data dir: {e}")))?;

        let key_pem = pem::encode(&pem::Pem::new("PRIVATE KEY", self.key_pkcs8.clone()));
        fs::write(data_dir.join(KEY_FILENAME), &key_pem)
            .map_err(|e| Error::Identity(format!("write key: {e}")))?;

        fs::write(data_dir.join(CERT_FILENAME), &self.cert_pem)
            .map_err(|e| Error::Identity(format!("write cert: {e}")))?;

        Ok(())
    }

    /// Build a rustls certificate chain.
    pub fn rustls_cert_chain(&self) -> Vec<CertificateDer<'static>> {
        vec![self.cert_der.clone()]
    }

    /// Build a rustls private key from stored PKCS#8 bytes.
    pub fn rustls_private_key(&self) -> PrivateKeyDer<'static> {
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(self.key_pkcs8.clone()))
    }
}

/// Peers directory within the data dir.
pub fn peers_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("peers")
}

/// Save a peer's certificate to disk.
pub fn pin_peer_cert(data_dir: &Path, peer_id: Uuid, cert_pem: &str) -> io::Result<()> {
    let dir = peers_dir(data_dir);
    fs::create_dir_all(&dir)?;
    fs::write(dir.join(format!("{peer_id}.cert")), cert_pem)?;
    Ok(())
}

/// Load all pinned peer certificates from disk.
pub fn load_pinned_certs(data_dir: &Path) -> io::Result<Vec<(Uuid, CertificateDer<'static>)>> {
    let dir = peers_dir(data_dir);
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut certs = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "cert") {
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if let Ok(uuid) = Uuid::parse_str(stem) {
                let pem_str = fs::read_to_string(&path)?;
                if let Ok(parsed) = pem::parse(&pem_str) {
                    certs.push((uuid, CertificateDer::from(parsed.contents().to_vec())));
                }
            }
        }
    }
    Ok(certs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_identity() {
        let id = Identity::generate().unwrap();
        assert!(!id.cert_pem.is_empty());
        assert!(!id.fingerprint.short().is_empty());
        assert_eq!(id.fingerprint.short().len(), 23); // 8 bytes * 2 hex + 7 colons
    }

    #[test]
    fn fingerprint_deterministic() {
        let id = Identity::generate().unwrap();
        let fp2 = Fingerprint::from_cert_der(id.cert_der.as_ref());
        assert_eq!(id.fingerprint, fp2);
    }

    #[test]
    fn save_and_load_roundtrip() {
        let tmp = tempfile::TempDir::new().unwrap();
        let id1 = Identity::generate().unwrap();
        id1.save(tmp.path()).unwrap();

        let id2 = Identity::load(
            &tmp.path().join(KEY_FILENAME),
            &tmp.path().join(CERT_FILENAME),
        )
        .unwrap();

        assert_eq!(id1.fingerprint, id2.fingerprint);
        assert_eq!(id1.cert_pem, id2.cert_pem);
    }

    #[test]
    fn pin_and_load_peer() {
        let tmp = tempfile::TempDir::new().unwrap();
        let peer = Identity::generate().unwrap();

        pin_peer_cert(tmp.path(), peer.id, &peer.cert_pem).unwrap();
        let loaded = load_pinned_certs(tmp.path()).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].0, peer.id);
    }
}
