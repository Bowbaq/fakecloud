use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StorageMode {
    #[default]
    Memory,
    Persistent,
}

#[derive(Clone, Debug)]
pub struct PersistenceConfig {
    pub mode: StorageMode,
    pub data_path: Option<PathBuf>,
    pub s3_cache_bytes: u64,
}

impl PersistenceConfig {
    pub fn memory() -> Self {
        Self {
            mode: StorageMode::Memory,
            data_path: None,
            s3_cache_bytes: 0,
        }
    }

    pub fn persistent(path: PathBuf, cache_bytes: u64) -> Self {
        Self {
            mode: StorageMode::Persistent,
            data_path: Some(path),
            s3_cache_bytes: cache_bytes,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        match self.mode {
            StorageMode::Persistent => {
                if self.data_path.is_none() {
                    return Err(
                        "--storage-mode=persistent requires --data-path to be set".to_string()
                    );
                }
            }
            StorageMode::Memory => {
                if self.data_path.is_some() {
                    return Err(
                        "--data-path is only valid with --storage-mode=persistent".to_string()
                    );
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_config_has_defaults() {
        let cfg = PersistenceConfig::memory();
        assert_eq!(cfg.mode, StorageMode::Memory);
        assert!(cfg.data_path.is_none());
        assert_eq!(cfg.s3_cache_bytes, 0);
    }

    #[test]
    fn persistent_config_stores_path() {
        let cfg = PersistenceConfig::persistent(PathBuf::from("/tmp/x"), 1024);
        assert_eq!(cfg.mode, StorageMode::Persistent);
        assert_eq!(
            cfg.data_path.as_deref(),
            Some(std::path::Path::new("/tmp/x"))
        );
        assert_eq!(cfg.s3_cache_bytes, 1024);
    }

    #[test]
    fn validate_memory_ok() {
        assert!(PersistenceConfig::memory().validate().is_ok());
    }

    #[test]
    fn validate_persistent_requires_path() {
        let cfg = PersistenceConfig {
            mode: StorageMode::Persistent,
            data_path: None,
            s3_cache_bytes: 0,
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_memory_with_data_path_errors() {
        let cfg = PersistenceConfig {
            mode: StorageMode::Memory,
            data_path: Some(PathBuf::from("/tmp")),
            s3_cache_bytes: 0,
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_persistent_with_path_ok() {
        assert!(PersistenceConfig::persistent(PathBuf::from("/tmp"), 0)
            .validate()
            .is_ok());
    }
}
