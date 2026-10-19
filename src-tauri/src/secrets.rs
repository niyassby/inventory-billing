//! Small secrets that must not sit in the database or in plain files: the device credential a
//! client got at pairing, the hub's TLS private key and the backup encryption key. The app uses
//! the OS keychain (Windows Credential Manager, macOS Keychain, Linux keyutils); tests use memory.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::error::{AppError, AppResult};

pub trait SecretStore: Send + Sync {
    fn get(&self, name: &str) -> AppResult<Option<String>>;
    fn set(&self, name: &str, value: &str) -> AppResult<()>;
    fn delete(&self, name: &str) -> AppResult<()>;
}

/// Names of the stored secrets.
pub const DEVICE_TOKEN: &str = "device-token";
pub const HUB_TLS_KEY: &str = "hub-tls-key";
pub const BACKUP_KEY: &str = "backup-key";

#[derive(Default)]
pub struct MemorySecrets(Mutex<HashMap<String, String>>);

impl SecretStore for MemorySecrets {
    fn get(&self, name: &str) -> AppResult<Option<String>> {
        Ok(self
            .0
            .lock()
            .map_err(AppError::internal_lock)?
            .get(name)
            .cloned())
    }

    fn set(&self, name: &str, value: &str) -> AppResult<()> {
        self.0
            .lock()
            .map_err(AppError::internal_lock)?
            .insert(name.to_string(), value.to_string());
        Ok(())
    }

    fn delete(&self, name: &str) -> AppResult<()> {
        self.0.lock().map_err(AppError::internal_lock)?.remove(name);
        Ok(())
    }
}

/// OS keychain, one entry per secret under the app's service name.
pub struct KeyringSecrets {
    service: String,
}

impl KeyringSecrets {
    pub fn new(service: &str) -> Self {
        Self {
            service: service.to_string(),
        }
    }

    fn entry(&self, name: &str) -> AppResult<keyring::Entry> {
        keyring::Entry::new(&self.service, name).map_err(AppError::internal)
    }
}

impl SecretStore for KeyringSecrets {
    fn get(&self, name: &str) -> AppResult<Option<String>> {
        match self.entry(name)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(AppError::internal(e)),
        }
    }

    fn set(&self, name: &str, value: &str) -> AppResult<()> {
        self.entry(name)?
            .set_password(value)
            .map_err(AppError::internal)
    }

    fn delete(&self, name: &str) -> AppResult<()> {
        match self.entry(name)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(AppError::internal(e)),
        }
    }
}
