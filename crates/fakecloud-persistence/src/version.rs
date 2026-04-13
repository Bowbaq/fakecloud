use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const FORMAT_VERSION: u32 = 1;
pub const VERSION_FILE_NAME: &str = "fakecloud.version.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatVersion {
    pub format_version: u32,
    pub fakecloud_version: String,
    pub created_at: String,
}

#[derive(Debug, Error)]
pub enum VersionError {
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("failed to serialize version file: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error(
        "persistence format version mismatch at {path}: on-disk format_version={on_disk}, binary expects {expected}. \
         Either point --data-path at a matching directory, or delete the directory to start fresh."
    )]
    FormatMismatch {
        path: PathBuf,
        on_disk: u32,
        expected: u32,
    },
}

fn version_file_path(dir: &Path) -> PathBuf {
    dir.join(VERSION_FILE_NAME)
}

pub fn write_version_file(dir: &Path, fakecloud_version: &str) -> Result<(), VersionError> {
    let path = version_file_path(dir);
    let value = FormatVersion {
        format_version: FORMAT_VERSION,
        fakecloud_version: fakecloud_version.to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    crate::atomic::write_atomic_toml(&path, &value).map_err(|source| VersionError::Io {
        path: path.clone(),
        source,
    })?;
    Ok(())
}

pub fn check_version_file(dir: &Path) -> Result<(), VersionError> {
    let path = version_file_path(dir);
    if !path.exists() {
        return Ok(());
    }
    let text = std::fs::read_to_string(&path).map_err(|source| VersionError::Io {
        path: path.clone(),
        source,
    })?;
    let parsed: FormatVersion = toml::from_str(&text).map_err(|source| VersionError::Parse {
        path: path.clone(),
        source,
    })?;
    if parsed.format_version != FORMAT_VERSION {
        return Err(VersionError::FormatMismatch {
            path,
            on_disk: parsed.format_version,
            expected: FORMAT_VERSION,
        });
    }
    Ok(())
}

pub fn ensure_version_file(dir: &Path, fakecloud_version: &str) -> Result<(), VersionError> {
    let path = version_file_path(dir);
    if path.exists() {
        check_version_file(dir)
    } else {
        write_version_file(dir, fakecloud_version)
    }
}
