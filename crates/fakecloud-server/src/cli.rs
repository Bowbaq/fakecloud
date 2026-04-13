use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use fakecloud_persistence::{PersistenceConfig, StorageMode};

#[derive(Clone, Copy, Debug, ValueEnum)]
#[clap(rename_all = "lowercase")]
pub(crate) enum StorageModeArg {
    Memory,
    Persistent,
}

impl From<StorageModeArg> for StorageMode {
    fn from(value: StorageModeArg) -> Self {
        match value {
            StorageModeArg::Memory => StorageMode::Memory,
            StorageModeArg::Persistent => StorageMode::Persistent,
        }
    }
}

const DEFAULT_S3_CACHE_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Parser)]
#[command(name = "fakecloud")]
#[command(about = "FakeCloud — local AWS cloud emulator")]
#[command(version)]
pub(crate) struct Cli {
    /// Listen address
    #[arg(long, default_value = "0.0.0.0:4566", env = "FAKECLOUD_ADDR")]
    pub addr: String,

    /// AWS region to advertise
    #[arg(long, default_value = "us-east-1", env = "FAKECLOUD_REGION")]
    pub region: String,

    /// AWS account ID to use
    #[arg(long, default_value = "123456789012", env = "FAKECLOUD_ACCOUNT_ID")]
    pub account_id: String,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info", env = "FAKECLOUD_LOG")]
    pub log_level: String,

    /// Storage mode. `memory` (default) keeps all state in RAM; `persistent`
    /// mirrors supported services to `--data-path` on disk.
    #[arg(
        long,
        value_enum,
        default_value_t = StorageModeArg::Memory,
        env = "FAKECLOUD_STORAGE_MODE",
    )]
    pub storage_mode: StorageModeArg,

    /// Directory to persist state to. Required when `--storage-mode=persistent`.
    #[arg(long, env = "FAKECLOUD_DATA_PATH")]
    pub data_path: Option<PathBuf>,

    /// In-memory LRU cache for S3 object bodies in persistent mode. Plain bytes,
    /// no SI/IEC suffix parsing. Default 256 MiB.
    #[arg(long, default_value_t = DEFAULT_S3_CACHE_BYTES, env = "FAKECLOUD_S3_CACHE_SIZE")]
    pub s3_cache_size: u64,
}

impl Cli {
    /// Derive the public-facing endpoint URL from the configured bind address.
    /// Wildcard hosts (``0.0.0.0`` / ``[::]``) are rewritten to ``localhost`` so
    /// the URL is meaningful when handed back to clients.
    pub fn endpoint_url(&self) -> String {
        let addr = &self.addr;
        let port = addr.rsplit(':').next().unwrap_or("4566");
        let host = addr.rsplit_once(':').map(|(h, _)| h).unwrap_or("0.0.0.0");
        let host = if host == "0.0.0.0" || host == "[::]" {
            "localhost"
        } else {
            host
        };
        format!("http://{host}:{port}")
    }

    pub fn persistence_config(&self) -> Result<PersistenceConfig, String> {
        let mode: StorageMode = self.storage_mode.into();
        let config = PersistenceConfig {
            mode,
            data_path: self.data_path.clone(),
            s3_cache_bytes: self.s3_cache_size,
        };
        config.validate()?;
        Ok(config)
    }
}
