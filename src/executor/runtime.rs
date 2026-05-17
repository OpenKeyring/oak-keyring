use crate::errors::mapping::vault::VaultError;
use crate::services::vault::Vault;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RuntimeVaultError {
    #[error("vault is locked")]
    Locked,
    #[error("vault database operation is pending")]
    Pending,
}

pub enum VaultRuntime {
    Locked,
    Pending,
    Open(Box<dyn Vault>),
}

impl VaultRuntime {
    pub fn locked() -> Self {
        Self::Locked
    }

    pub fn open(vault: Box<dyn Vault>) -> Self {
        Self::Open(vault)
    }

    pub fn is_open(&self) -> bool {
        matches!(self, Self::Open(_))
    }

    pub fn open_vault(&self) -> Result<&dyn Vault, RuntimeVaultError> {
        match self {
            Self::Open(vault) => Ok(vault.as_ref()),
            Self::Locked => Err(RuntimeVaultError::Locked),
            Self::Pending => Err(RuntimeVaultError::Pending),
        }
    }

    pub fn open_vault_mut(&mut self) -> Result<&mut dyn Vault, RuntimeVaultError> {
        match self {
            Self::Open(vault) => Ok(vault.as_mut()),
            Self::Locked => Err(RuntimeVaultError::Locked),
            Self::Pending => Err(RuntimeVaultError::Pending),
        }
    }

    pub fn take_open(&mut self) -> Option<Box<dyn Vault>> {
        match std::mem::replace(self, Self::Locked) {
            Self::Open(vault) => Some(vault),
            other => {
                *self = other;
                None
            }
        }
    }
}

impl From<RuntimeVaultError> for VaultError {
    fn from(_: RuntimeVaultError) -> Self {
        VaultError::NotUnlocked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locked_runtime_rejects_vault_access() {
        let runtime = VaultRuntime::Locked;
        assert!(runtime.open_vault().is_err());
        // Verify the error type
        match runtime.open_vault() {
            Err(RuntimeVaultError::Locked) => {}
            _ => panic!("expected Locked error"),
        }
    }

    #[test]
    fn open_runtime_accepts_vault_access() {
        let conn = crate::db::schema::init_db_in_memory();
        let vault = crate::services::vault::VaultServiceImpl::new(conn);
        let runtime = VaultRuntime::Open(Box::new(vault));
        assert!(runtime.open_vault().is_ok());
    }

    #[test]
    fn pending_runtime_rejects_vault_access() {
        let runtime = VaultRuntime::Pending;
        assert!(runtime.open_vault().is_err());
        match runtime.open_vault() {
            Err(RuntimeVaultError::Pending) => {}
            _ => panic!("expected Pending error"),
        }
    }

    #[test]
    fn is_open_returns_true_for_open() {
        let conn = crate::db::schema::init_db_in_memory();
        let vault = crate::services::vault::VaultServiceImpl::new(conn);
        let runtime = VaultRuntime::Open(Box::new(vault));
        assert!(runtime.is_open());
    }

    #[test]
    fn is_open_returns_false_for_locked() {
        let runtime = VaultRuntime::Locked;
        assert!(!runtime.is_open());
    }

    #[test]
    fn runtime_vault_error_maps_to_not_unlocked() {
        let err: VaultError = RuntimeVaultError::Locked.into();
        assert!(matches!(err, VaultError::NotUnlocked));
        let err: VaultError = RuntimeVaultError::Pending.into();
        assert!(matches!(err, VaultError::NotUnlocked));
    }

    #[test]
    fn locked_runtime_rejects_mut_vault_access() {
        let mut runtime = VaultRuntime::Locked;
        assert!(runtime.open_vault_mut().is_err());
        match runtime.open_vault_mut() {
            Err(RuntimeVaultError::Locked) => {}
            _ => panic!("expected Locked error"),
        }
    }

    #[test]
    fn take_open_returns_vault_from_open_runtime() {
        let conn = crate::db::schema::init_db_in_memory();
        let vault = crate::services::vault::VaultServiceImpl::new(conn);
        let mut runtime = VaultRuntime::Open(Box::new(vault));
        assert!(runtime.take_open().is_some());
        // After take, runtime is locked
        assert!(!runtime.is_open());
    }

    #[test]
    fn take_open_returns_none_from_locked_runtime() {
        let mut runtime = VaultRuntime::Locked;
        assert!(runtime.take_open().is_none());
        assert!(!runtime.is_open());
    }
}
