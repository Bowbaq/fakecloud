use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;

fn tmp_path(path: &Path) -> PathBuf {
    let mut os = path.as_os_str().to_owned();
    os.push(".tmp");
    PathBuf::from(os)
}

fn fsync_parent(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            let dir = File::open(parent)?;
            dir.sync_all()?;
        }
    }
    Ok(())
}

fn write_atomic_bytes_inner(tmp: &Path, path: &Path, bytes: &[u8]) -> io::Result<()> {
    {
        let mut f = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(tmp, path)?;
    fsync_parent(path)?;
    Ok(())
}

pub fn write_atomic_bytes(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = tmp_path(path);
    match write_atomic_bytes_inner(&tmp, path, bytes) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

pub fn write_atomic_toml<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    let text = toml::to_string_pretty(value)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    write_atomic_bytes(path, text.as_bytes())
}

fn write_atomic_from_file_inner(src: &Path, dst: &Path) -> io::Result<()> {
    {
        let f = File::open(src)?;
        f.sync_all()?;
    }
    std::fs::rename(src, dst)?;
    fsync_parent(dst)?;
    Ok(())
}

pub fn write_atomic_from_file(src: &Path, dst: &Path) -> io::Result<()> {
    match write_atomic_from_file_inner(src, dst) {
        Ok(()) => Ok(()),
        Err(e) => {
            // Best-effort cleanup: remove any stray tmp the caller might see.
            let tmp = tmp_path(dst);
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_write_leaves_no_tmp() {
        // Writing into a non-existent parent directory should fail without
        // leaving a lingering `.tmp` sibling. Use a tempdir so the test is
        // hermetic.
        let tmp = tempfile::tempdir().unwrap();
        let bogus = tmp.path().join("does/not/exist/target.bin");
        let err = write_atomic_bytes(&bogus, b"hello").unwrap_err();
        let tmp_sibling = tmp_path(&bogus);
        assert!(!tmp_sibling.exists(), "stray tmp: {:?}", tmp_sibling);
        let _ = err;
    }
}
