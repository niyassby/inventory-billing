//! The hub's TLS identity: a self-signed certificate (ECDSA P-256) made on first start. The
//! certificate is a file next to the database; its private key is kept in the OS keychain. The
//! SHA-256 fingerprint is what counters pin, so it must stay the same across restarts.

use std::fs;

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use sha2::{Digest, Sha256};

use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::secrets::HUB_TLS_KEY;

pub struct Identity {
    pub cert_der: Vec<u8>,
    pub key_der: Vec<u8>,
    pub fingerprint: String,
}

pub fn fingerprint(cert_der: &[u8]) -> String {
    hex::encode(Sha256::digest(cert_der))
}

pub fn provider() -> std::sync::Arc<rustls::crypto::CryptoProvider> {
    std::sync::Arc::new(rustls::crypto::ring::default_provider())
}

impl Identity {
    pub fn server_config(&self) -> AppResult<rustls::ServerConfig> {
        rustls::ServerConfig::builder_with_provider(provider())
            .with_safe_default_protocol_versions()
            .map_err(AppError::internal)?
            .with_no_client_auth()
            .with_single_cert(
                vec![CertificateDer::from(self.cert_der.clone())],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(self.key_der.clone())),
            )
            .map_err(AppError::internal)
    }
}

pub fn load_or_create(state: &AppState) -> AppResult<Identity> {
    let dir = state.data_dir().join("hub");
    let cert_path = dir.join("hub-cert.der");
    if let (Ok(cert_der), Some(key_hex)) = (fs::read(&cert_path), state.secrets.get(HUB_TLS_KEY)?) {
        if let Ok(key_der) = hex::decode(key_hex) {
            let identity = Identity {
                fingerprint: fingerprint(&cert_der),
                cert_der,
                key_der,
            };
            if identity.server_config().is_ok() {
                return Ok(identity);
            }
        }
        tracing::warn!(
            "the hub certificate or key is unusable; making a new one (counters must pair again)"
        );
    }
    let mut names = vec!["inventory-hub".to_string(), "localhost".to_string()];
    if let Ok(host) = std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")) {
        names.push(host.to_lowercase());
    }
    let certified = rcgen::generate_simple_self_signed(names).map_err(AppError::internal)?;
    let cert_der = certified.cert.der().to_vec();
    let key_der = certified.key_pair.serialize_der();
    fs::create_dir_all(&dir).map_err(AppError::internal)?;
    fs::write(&cert_path, &cert_der).map_err(AppError::internal)?;
    state.secrets.set(HUB_TLS_KEY, &hex::encode(&key_der))?;
    Ok(Identity {
        fingerprint: fingerprint(&cert_der),
        cert_der,
        key_der,
    })
}
