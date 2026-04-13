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
