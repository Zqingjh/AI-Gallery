//! 系统凭据库适配器。
//!
//! 此模块只接收不透明的凭据引用，不会把密钥写入工作区、SQLite 或日志。

const SERVICE_NAME: &str = "ai-gallery";
const MAX_CREDENTIAL_ID_LENGTH: usize = 128;
const MAX_SECRET_LENGTH: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SecureCredentialStoreError {
    InvalidReference,
    Unavailable,
}

pub(crate) trait SecureCredentialStore {
    fn get(&self, credential_id: &str) -> Result<Option<String>, SecureCredentialStoreError>;
    fn set(&self, credential_id: &str, secret: &str) -> Result<(), SecureCredentialStoreError>;
    fn delete(&self, credential_id: &str) -> Result<(), SecureCredentialStoreError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct SystemCredentialStore;

impl SystemCredentialStore {
    pub(crate) fn new() -> Self {
        Self
    }

    fn entry(&self, credential_id: &str) -> Result<keyring::Entry, SecureCredentialStoreError> {
        validate_credential_id(credential_id)?;
        keyring::Entry::new(SERVICE_NAME, credential_id)
            .map_err(|_| SecureCredentialStoreError::Unavailable)
    }
}

impl SecureCredentialStore for SystemCredentialStore {
    fn get(&self, credential_id: &str) -> Result<Option<String>, SecureCredentialStoreError> {
        let entry = self.entry(credential_id)?;
        match entry.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(SecureCredentialStoreError::Unavailable),
        }
    }

    fn set(&self, credential_id: &str, secret: &str) -> Result<(), SecureCredentialStoreError> {
        validate_secret(secret)?;
        self.entry(credential_id)?
            .set_password(secret)
            .map_err(|_| SecureCredentialStoreError::Unavailable)
    }

    fn delete(&self, credential_id: &str) -> Result<(), SecureCredentialStoreError> {
        let entry = self.entry(credential_id)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(SecureCredentialStoreError::Unavailable),
        }
    }
}

fn validate_credential_id(value: &str) -> Result<(), SecureCredentialStoreError> {
    let valid = !value.is_empty()
        && value.len() <= MAX_CREDENTIAL_ID_LENGTH
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
    if valid {
        Ok(())
    } else {
        Err(SecureCredentialStoreError::InvalidReference)
    }
}

fn validate_secret(value: &str) -> Result<(), SecureCredentialStoreError> {
    if !value.trim().is_empty() && value.len() <= MAX_SECRET_LENGTH {
        Ok(())
    } else {
        Err(SecureCredentialStoreError::InvalidReference)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SecureCredentialStoreError, SystemCredentialStore, validate_credential_id, validate_secret,
    };

    #[test]
    fn credential_reference_is_bounded_and_non_sensitive() {
        assert!(validate_credential_id("provider_ABC-123").is_ok());
        assert_eq!(
            validate_credential_id("provider/ABC"),
            Err(SecureCredentialStoreError::InvalidReference)
        );
    }

    #[test]
    fn secret_must_not_be_empty() {
        assert!(validate_secret("not-a-real-secret").is_ok());
        assert_eq!(
            validate_secret("  "),
            Err(SecureCredentialStoreError::InvalidReference)
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_uses_native_credential_store() {
        let entry = SystemCredentialStore::new()
            .entry("provider-diagnostic")
            .expect("应能创建 Windows 凭据条目");
        assert!(
            entry
                .get_credential()
                .downcast_ref::<keyring::windows::WinCredential>()
                .is_some(),
            "Windows 构建必须使用系统凭据管理器，而非仅进程内的 mock 存储"
        );
    }
}
