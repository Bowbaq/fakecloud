use std::collections::HashMap;
use std::path::PathBuf;

use bytes::Bytes;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct S3State {
    #[serde(default)]
    pub account_id: String,
    #[serde(default)]
    pub region: String,
    #[serde(default)]
    pub buckets: HashMap<String, BucketSnapshot>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct BucketSnapshot {
    pub meta: BucketMeta,
    #[serde(default)]
    pub objects: HashMap<String, LoadedObject>,
    #[serde(default)]
    pub object_versions: HashMap<String, Vec<LoadedObject>>,
    #[serde(default)]
    pub subresources: HashMap<String, String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct LoadedObject {
    pub meta: ObjectMeta,
    #[serde(default)]
    pub body: BodyRef,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct BucketMeta {
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_time")]
    pub creation_date: DateTime<Utc>,
    #[serde(default)]
    pub region: String,
    #[serde(default)]
    pub versioning: Option<String>,
    #[serde(default)]
    pub acl: Option<String>,
    #[serde(default)]
    pub acl_owner_id: String,
    #[serde(default)]
    pub accelerate_status: Option<String>,
    #[serde(default)]
    pub eventbridge_enabled: bool,
}

fn default_time() -> DateTime<Utc> {
    DateTime::<Utc>::MIN_UTC
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct AclGrantSnapshot {
    pub grantee_type: String,
    #[serde(default)]
    pub grantee_id: Option<String>,
    #[serde(default)]
    pub grantee_display_name: Option<String>,
    #[serde(default)]
    pub grantee_uri: Option<String>,
    pub permission: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ObjectMeta {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub content_type: String,
    #[serde(default)]
    pub etag: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default = "default_time")]
    pub last_modified: DateTime<Utc>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    #[serde(default)]
    pub tags: HashMap<String, String>,
    #[serde(default)]
    pub storage_class: String,
    #[serde(default)]
    pub acl_grants: Vec<AclGrantSnapshot>,
    #[serde(default)]
    pub acl_owner_id: Option<String>,
    #[serde(default)]
    pub parts_count: Option<u32>,
    #[serde(default)]
    pub part_sizes: Option<Vec<(u32, u64)>>,
    #[serde(default)]
    pub sse_algorithm: Option<String>,
    #[serde(default)]
    pub sse_kms_key_id: Option<String>,
    #[serde(default)]
    pub bucket_key_enabled: Option<bool>,
    #[serde(default)]
    pub version_id: Option<String>,
    #[serde(default)]
    pub is_delete_marker: bool,
    #[serde(default)]
    pub restore_ongoing: Option<bool>,
    #[serde(default)]
    pub restore_expiry: Option<String>,
    #[serde(default)]
    pub checksum_algorithm: Option<String>,
    #[serde(default)]
    pub checksum_value: Option<String>,
    #[serde(default)]
    pub lock_mode: Option<String>,
    #[serde(default)]
    pub lock_retain_until: Option<DateTime<Utc>>,
    #[serde(default)]
    pub lock_legal_hold: Option<String>,
    #[serde(default)]
    pub content_encoding: Option<String>,
    #[serde(default)]
    pub website_redirect_location: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct MpuInit {
    pub upload_id: String,
    pub key: String,
    #[serde(default = "default_time")]
    pub initiated: DateTime<Utc>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    #[serde(default)]
    pub content_type: String,
    #[serde(default)]
    pub storage_class: String,
    #[serde(default)]
    pub sse_algorithm: Option<String>,
    #[serde(default)]
    pub sse_kms_key_id: Option<String>,
    #[serde(default)]
    pub tagging: Option<String>,
    #[serde(default)]
    pub acl_grants: Vec<AclGrantSnapshot>,
    #[serde(default)]
    pub checksum_algorithm: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct UploadPartMeta {
    pub part_number: u32,
    pub etag: String,
    pub size: u64,
    #[serde(default = "default_time")]
    pub last_modified: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BucketSubresource {
    Tags,
    Lifecycle,
    Cors,
    Policy,
    Notification,
    Logging,
    Website,
    PublicAccessBlock,
    ObjectLock,
    Replication,
    Ownership,
    Inventory,
    Encryption,
    Versioning,
    Acl,
    Accelerate,
}

pub const ALL_SUBRESOURCES: &[BucketSubresource] = &[
    BucketSubresource::Tags,
    BucketSubresource::Lifecycle,
    BucketSubresource::Cors,
    BucketSubresource::Policy,
    BucketSubresource::Notification,
    BucketSubresource::Logging,
    BucketSubresource::Website,
    BucketSubresource::PublicAccessBlock,
    BucketSubresource::ObjectLock,
    BucketSubresource::Replication,
    BucketSubresource::Ownership,
    BucketSubresource::Inventory,
    BucketSubresource::Encryption,
    BucketSubresource::Versioning,
    BucketSubresource::Acl,
    BucketSubresource::Accelerate,
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BodyRef {
    #[serde(skip)]
    Memory(Bytes),
    Disk {
        bucket: String,
        key: String,
        #[serde(default)]
        version: Option<String>,
        path: PathBuf,
        size: u64,
    },
}

impl BodyRef {
    pub fn size(&self) -> u64 {
        match self {
            BodyRef::Memory(b) => b.len() as u64,
            BodyRef::Disk { size, .. } => *size,
        }
    }
}

impl Default for BodyRef {
    fn default() -> Self {
        BodyRef::Memory(Bytes::new())
    }
}

#[derive(Debug)]
pub enum BodySource {
    Bytes(Bytes),
    File(PathBuf),
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serde(String),
    #[error("not supported by this store")]
    NotSupported,
    #[error("{0}")]
    Other(String),
}

pub type StoreResult<T> = Result<T, StoreError>;

pub trait S3Store: Send + Sync {
    fn load(&self) -> StoreResult<S3State>;

    fn put_bucket_meta(&self, bucket: &str, meta: &BucketMeta) -> StoreResult<()>;
    fn put_bucket_subresource(
        &self,
        bucket: &str,
        kind: BucketSubresource,
        payload: &str,
    ) -> StoreResult<()>;
    fn delete_bucket_subresource(
        &self,
        bucket: &str,
        kind: BucketSubresource,
    ) -> StoreResult<()>;
    fn delete_bucket(&self, bucket: &str) -> StoreResult<()>;

    fn put_object(
        &self,
        bucket: &str,
        key: &str,
        version: Option<&str>,
        body: BodySource,
        meta: &ObjectMeta,
    ) -> StoreResult<BodyRef>;
    fn put_object_meta(
        &self,
        bucket: &str,
        key: &str,
        version: Option<&str>,
        meta: &ObjectMeta,
    ) -> StoreResult<()>;
    fn delete_object(
        &self,
        bucket: &str,
        key: &str,
        version: Option<&str>,
    ) -> StoreResult<()>;
    fn open_object_body(&self, body: &BodyRef) -> StoreResult<Bytes>;

    fn mpu_create(&self, bucket: &str, upload_id: &str, init: &MpuInit) -> StoreResult<()>;
    fn mpu_put_part(
        &self,
        bucket: &str,
        upload_id: &str,
        part_number: u32,
        body: BodySource,
        etag: &str,
    ) -> StoreResult<()>;
    fn mpu_abort(&self, bucket: &str, upload_id: &str) -> StoreResult<()>;
    fn mpu_complete(
        &self,
        bucket: &str,
        upload_id: &str,
        final_key: &str,
        version: Option<&str>,
        meta: &ObjectMeta,
    ) -> StoreResult<BodyRef>;
}

pub struct MemoryS3Store;

impl MemoryS3Store {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MemoryS3Store {
    fn default() -> Self {
        Self::new()
    }
}

impl S3Store for MemoryS3Store {
    fn load(&self) -> StoreResult<S3State> {
        Ok(S3State::default())
    }

    fn put_bucket_meta(&self, _bucket: &str, _meta: &BucketMeta) -> StoreResult<()> {
        Ok(())
    }
    fn put_bucket_subresource(
        &self,
        _bucket: &str,
        _kind: BucketSubresource,
        _payload: &str,
    ) -> StoreResult<()> {
        Ok(())
    }
    fn delete_bucket_subresource(
        &self,
        _bucket: &str,
        _kind: BucketSubresource,
    ) -> StoreResult<()> {
        Ok(())
    }
    fn delete_bucket(&self, _bucket: &str) -> StoreResult<()> {
        Ok(())
    }

    fn put_object(
        &self,
        _bucket: &str,
        _key: &str,
        _version: Option<&str>,
        body: BodySource,
        _meta: &ObjectMeta,
    ) -> StoreResult<BodyRef> {
        match body {
            BodySource::Bytes(b) => Ok(BodyRef::Memory(b)),
            BodySource::File(_) => Err(StoreError::NotSupported),
        }
    }
    fn put_object_meta(
        &self,
        _bucket: &str,
        _key: &str,
        _version: Option<&str>,
        _meta: &ObjectMeta,
    ) -> StoreResult<()> {
        Ok(())
    }
    fn delete_object(
        &self,
        _bucket: &str,
        _key: &str,
        _version: Option<&str>,
    ) -> StoreResult<()> {
        Ok(())
    }
    fn open_object_body(&self, body: &BodyRef) -> StoreResult<Bytes> {
        match body {
            BodyRef::Memory(b) => Ok(b.clone()),
            BodyRef::Disk { .. } => {
                panic!("MemoryS3Store cannot open Disk-backed BodyRef")
            }
        }
    }

    fn mpu_create(&self, _bucket: &str, _upload_id: &str, _init: &MpuInit) -> StoreResult<()> {
        Ok(())
    }
    fn mpu_put_part(
        &self,
        _bucket: &str,
        _upload_id: &str,
        _part_number: u32,
        _body: BodySource,
        _etag: &str,
    ) -> StoreResult<()> {
        Ok(())
    }
    fn mpu_abort(&self, _bucket: &str, _upload_id: &str) -> StoreResult<()> {
        Ok(())
    }
    fn mpu_complete(
        &self,
        _bucket: &str,
        _upload_id: &str,
        _final_key: &str,
        _version: Option<&str>,
        _meta: &ObjectMeta,
    ) -> StoreResult<BodyRef> {
        Ok(BodyRef::Memory(Bytes::new()))
    }
}

pub struct DiskS3Store {
    root: PathBuf,
    cache: std::sync::Arc<crate::cache::BodyCache>,
}

impl DiskS3Store {
    pub fn new(root: PathBuf, cache: std::sync::Arc<crate::cache::BodyCache>) -> Self {
        Self { root, cache }
    }

    fn buckets_dir(&self) -> PathBuf {
        self.root.join("buckets")
    }

    fn bucket_dir(&self, bucket: &str) -> PathBuf {
        self.buckets_dir()
            .join(crate::key_escape::escape_key_segment(bucket))
    }

    fn object_dir(&self, bucket: &str, key: &str) -> PathBuf {
        self.bucket_dir(bucket)
            .join("objects")
            .join(crate::key_escape::escape_key_segment(key))
    }

    fn version_tag(version: Option<&str>) -> String {
        version.unwrap_or("null").to_string()
    }

    fn object_paths(
        &self,
        bucket: &str,
        key: &str,
        version: Option<&str>,
    ) -> (PathBuf, PathBuf, PathBuf) {
        let dir = self.object_dir(bucket, key);
        let tag = Self::version_tag(version);
        let bin = dir.join(format!("{}.bin", tag));
        let toml = dir.join(format!("{}.toml", tag));
        (dir, bin, toml)
    }

    fn subresource_filename(kind: BucketSubresource) -> &'static str {
        match kind {
            BucketSubresource::Tags => "tags.toml",
            BucketSubresource::Lifecycle => "lifecycle.toml",
            BucketSubresource::Cors => "cors.toml",
            BucketSubresource::Policy => "policy.toml",
            BucketSubresource::Notification => "notification.toml",
            BucketSubresource::Logging => "logging.toml",
            BucketSubresource::Website => "website.toml",
            BucketSubresource::PublicAccessBlock => "public_access_block.toml",
            BucketSubresource::ObjectLock => "object_lock.toml",
            BucketSubresource::Replication => "replication.toml",
            BucketSubresource::Ownership => "ownership.toml",
            BucketSubresource::Inventory => "inventory.toml",
            BucketSubresource::Encryption => "encryption.toml",
            BucketSubresource::Versioning => "versioning.toml",
            BucketSubresource::Acl => "acl.toml",
            BucketSubresource::Accelerate => "accelerate.toml",
        }
    }

    fn cleanup_empty(dir: &std::path::Path) {
        let _ = std::fs::remove_dir(dir);
    }
}

fn io_other(msg: impl Into<String>) -> StoreError {
    StoreError::Other(msg.into())
}

impl S3Store for DiskS3Store {
    fn load(&self) -> StoreResult<S3State> {
        let mut state = S3State::default();
        let buckets_dir = self.buckets_dir();
        if !buckets_dir.exists() {
            return Ok(state);
        }
        for entry in std::fs::read_dir(&buckets_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let bdir = entry.path();
            let meta_path = bdir.join("meta.toml");
            if !meta_path.exists() {
                continue;
            }
            let meta_text = std::fs::read_to_string(&meta_path)?;
            let mut meta: BucketMeta =
                toml::from_str(&meta_text).map_err(|e| StoreError::Serde(e.to_string()))?;
            let mut snap = BucketSnapshot {
                meta: meta.clone(),
                objects: HashMap::new(),
                object_versions: HashMap::new(),
                subresources: HashMap::new(),
            };

            for kind in ALL_SUBRESOURCES {
                let fname = Self::subresource_filename(*kind);
                let path = bdir.join(fname);
                if path.exists() {
                    let text = std::fs::read_to_string(&path)?;
                    if *kind == BucketSubresource::Versioning && snap.meta.versioning.is_none() {
                        let stripped = text.trim();
                        if !stripped.is_empty() {
                            snap.meta.versioning = Some(stripped.to_string());
                            meta.versioning = snap.meta.versioning.clone();
                        }
                    }
                    snap.subresources.insert(fname.to_string(), text);
                }
            }

            let objects_root = bdir.join("objects");
            if objects_root.exists() {
                for okey_entry in std::fs::read_dir(&objects_root)? {
                    let okey_entry = okey_entry?;
                    if !okey_entry.file_type()?.is_dir() {
                        continue;
                    }
                    let key_dir = okey_entry.path();
                    let mut versioned: Vec<LoadedObject> = Vec::new();
                    let mut key_name: Option<String> = None;
                    for version_entry in std::fs::read_dir(&key_dir)? {
                        let version_entry = version_entry?;
                        let path = version_entry.path();
                        let Some(fname) = path.file_name().and_then(|s| s.to_str()) else {
                            continue;
                        };
                        if !fname.ends_with(".toml") {
                            continue;
                        }
                        let version_tag = &fname[..fname.len() - 5];
                        let toml_text = std::fs::read_to_string(&path)?;
                        let obj_meta: ObjectMeta = toml::from_str(&toml_text)
                            .map_err(|e| StoreError::Serde(e.to_string()))?;
                        let bin_path = key_dir.join(format!("{}.bin", version_tag));
                        let (body, size) = if obj_meta.is_delete_marker {
                            (BodyRef::Memory(Bytes::new()), 0u64)
                        } else if bin_path.exists() {
                            let sz = std::fs::metadata(&bin_path)?.len();
                            (
                                BodyRef::Disk {
                                    bucket: meta.name.clone(),
                                    key: obj_meta.key.clone(),
                                    version: if version_tag == "null" {
                                        None
                                    } else {
                                        Some(version_tag.to_string())
                                    },
                                    path: bin_path,
                                    size: sz,
                                },
                                sz,
                            )
                        } else {
                            (BodyRef::Memory(Bytes::new()), 0u64)
                        };
                        let _ = size;
                        key_name.get_or_insert_with(|| obj_meta.key.clone());
                        if version_tag == "null" && obj_meta.version_id.is_none() {
                            snap.objects.insert(
                                obj_meta.key.clone(),
                                LoadedObject {
                                    meta: obj_meta,
                                    body,
                                },
                            );
                        } else {
                            versioned.push(LoadedObject {
                                meta: obj_meta,
                                body,
                            });
                        }
                    }
                    if !versioned.is_empty() {
                        versioned.sort_by(|a, b| a.meta.last_modified.cmp(&b.meta.last_modified));
                        if let Some(key) = key_name {
                            snap.object_versions.insert(key, versioned);
                        }
                    }
                }
            }

            // TODO(phase-6): mpu/ directory is ignored.
            let _ = bdir.join("mpu");

            state.buckets.insert(meta.name.clone(), snap);
        }
        Ok(state)
    }

    fn put_bucket_meta(&self, bucket: &str, meta: &BucketMeta) -> StoreResult<()> {
        let dir = self.bucket_dir(bucket);
        std::fs::create_dir_all(&dir)?;
        crate::atomic::write_atomic_toml(&dir.join("meta.toml"), meta)?;
        Ok(())
    }

    fn put_bucket_subresource(
        &self,
        bucket: &str,
        kind: BucketSubresource,
        payload: &str,
    ) -> StoreResult<()> {
        let dir = self.bucket_dir(bucket);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(Self::subresource_filename(kind));
        crate::atomic::write_atomic_bytes(&path, payload.as_bytes())?;
        Ok(())
    }

    fn delete_bucket_subresource(
        &self,
        bucket: &str,
        kind: BucketSubresource,
    ) -> StoreResult<()> {
        let path = self.bucket_dir(bucket).join(Self::subresource_filename(kind));
        match std::fs::remove_file(&path) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn delete_bucket(&self, bucket: &str) -> StoreResult<()> {
        let dir = self.bucket_dir(bucket);
        match std::fs::remove_dir_all(&dir) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn put_object(
        &self,
        bucket: &str,
        key: &str,
        version: Option<&str>,
        body: BodySource,
        meta: &ObjectMeta,
    ) -> StoreResult<BodyRef> {
        let (dir, bin_path, toml_path) = self.object_paths(bucket, key, version);
        std::fs::create_dir_all(&dir)?;

        if meta.is_delete_marker {
            crate::atomic::write_atomic_toml(&toml_path, meta)?;
            return Ok(BodyRef::Memory(Bytes::new()));
        }

        let size: u64;
        let bytes_for_cache: Option<Bytes>;
        match body {
            BodySource::Bytes(b) => {
                size = b.len() as u64;
                crate::atomic::write_atomic_bytes(&bin_path, &b)?;
                bytes_for_cache = Some(b);
            }
            BodySource::File(src) => {
                let src_size = std::fs::metadata(&src)?.len();
                size = src_size;
                crate::atomic::write_atomic_from_file(&src, &bin_path)?;
                bytes_for_cache = None;
            }
        }

        crate::atomic::write_atomic_toml(&toml_path, meta)?;

        let body_key = crate::cache::BodyKey::new(
            bucket.to_string(),
            key.to_string(),
            version.map(|s| s.to_string()),
        );
        if let Some(b) = bytes_for_cache {
            self.cache.insert(body_key, b);
        } else {
            self.cache.invalidate(&crate::cache::BodyKey::new(
                bucket.to_string(),
                key.to_string(),
                version.map(|s| s.to_string()),
            ));
        }

        Ok(BodyRef::Disk {
            bucket: bucket.to_string(),
            key: key.to_string(),
            version: version.map(|s| s.to_string()),
            path: bin_path,
            size,
        })
    }

    fn put_object_meta(
        &self,
        bucket: &str,
        key: &str,
        version: Option<&str>,
        meta: &ObjectMeta,
    ) -> StoreResult<()> {
        let (dir, _bin, toml_path) = self.object_paths(bucket, key, version);
        std::fs::create_dir_all(&dir)?;
        crate::atomic::write_atomic_toml(&toml_path, meta)?;
        Ok(())
    }

    fn delete_object(
        &self,
        bucket: &str,
        key: &str,
        version: Option<&str>,
    ) -> StoreResult<()> {
        let (dir, bin_path, toml_path) = self.object_paths(bucket, key, version);
        for p in [&bin_path, &toml_path] {
            match std::fs::remove_file(p) {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        Self::cleanup_empty(&dir);

        self.cache.invalidate(&crate::cache::BodyKey::new(
            bucket.to_string(),
            key.to_string(),
            version.map(|s| s.to_string()),
        ));
        Ok(())
    }

    fn open_object_body(&self, body: &BodyRef) -> StoreResult<Bytes> {
        match body {
            BodyRef::Memory(b) => Ok(b.clone()),
            BodyRef::Disk {
                bucket,
                key,
                version,
                path,
                size: _,
            } => {
                let body_key = crate::cache::BodyKey::new(
                    bucket.clone(),
                    key.clone(),
                    version.clone(),
                );
                if let Some(bytes) = self.cache.get(&body_key) {
                    return Ok(bytes);
                }
                let bytes = Bytes::from(std::fs::read(path)?);
                self.cache.insert(body_key, bytes.clone());
                Ok(bytes)
            }
        }
    }

    fn mpu_create(&self, _bucket: &str, _upload_id: &str, _init: &MpuInit) -> StoreResult<()> {
        // TODO(phase-6): resumable multipart persistence.
        Err(io_other("multipart persistence not yet implemented — phase 6"))
    }
    fn mpu_put_part(
        &self,
        _bucket: &str,
        _upload_id: &str,
        _part_number: u32,
        _body: BodySource,
        _etag: &str,
    ) -> StoreResult<()> {
        // TODO(phase-6): resumable multipart persistence.
        Err(io_other("multipart persistence not yet implemented — phase 6"))
    }
    fn mpu_abort(&self, _bucket: &str, _upload_id: &str) -> StoreResult<()> {
        // TODO(phase-6): resumable multipart persistence.
        Err(io_other("multipart persistence not yet implemented — phase 6"))
    }
    fn mpu_complete(
        &self,
        _bucket: &str,
        _upload_id: &str,
        _final_key: &str,
        _version: Option<&str>,
        _meta: &ObjectMeta,
    ) -> StoreResult<BodyRef> {
        // TODO(phase-6): resumable multipart persistence.
        Err(io_other("multipart persistence not yet implemented — phase 6"))
    }
}

#[cfg(test)]
mod disk_tests {
    use super::*;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn new_store(tmp: &TempDir) -> DiskS3Store {
        let cache = Arc::new(crate::cache::BodyCache::new(1024 * 1024));
        DiskS3Store::new(tmp.path().to_path_buf(), cache)
    }

    fn new_store_with_cache(tmp: &TempDir, cap: u64) -> (DiskS3Store, Arc<crate::cache::BodyCache>) {
        let cache = Arc::new(crate::cache::BodyCache::new(cap));
        (
            DiskS3Store::new(tmp.path().to_path_buf(), cache.clone()),
            cache,
        )
    }

    fn sample_meta(key: &str, size: u64) -> ObjectMeta {
        ObjectMeta {
            key: key.to_string(),
            content_type: "application/octet-stream".to_string(),
            etag: "etag".to_string(),
            size,
            ..Default::default()
        }
    }

    #[test]
    fn put_bucket_meta_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let store = new_store(&tmp);
        let meta = BucketMeta {
            name: "b1".to_string(),
            region: "us-east-1".to_string(),
            versioning: Some("Enabled".to_string()),
            ..Default::default()
        };
        store.put_bucket_meta("b1", &meta).unwrap();
        let loaded = store.load().unwrap();
        let snap = loaded.buckets.get("b1").unwrap();
        assert_eq!(snap.meta.name, "b1");
        assert_eq!(snap.meta.region, "us-east-1");
        assert_eq!(snap.meta.versioning.as_deref(), Some("Enabled"));
    }

    #[test]
    fn put_bucket_subresource_each_variant_writes_file() {
        let tmp = TempDir::new().unwrap();
        let store = new_store(&tmp);
        store
            .put_bucket_meta("b", &BucketMeta {
                name: "b".to_string(),
                ..Default::default()
            })
            .unwrap();
        let variants = [
            BucketSubresource::Tags,
            BucketSubresource::Lifecycle,
            BucketSubresource::Cors,
            BucketSubresource::Policy,
            BucketSubresource::Notification,
            BucketSubresource::Logging,
            BucketSubresource::Website,
            BucketSubresource::PublicAccessBlock,
            BucketSubresource::ObjectLock,
            BucketSubresource::Replication,
            BucketSubresource::Ownership,
            BucketSubresource::Inventory,
            BucketSubresource::Encryption,
            BucketSubresource::Versioning,
            BucketSubresource::Acl,
            BucketSubresource::Accelerate,
        ];
        for v in variants {
            store.put_bucket_subresource("b", v, "payload=true").unwrap();
            let file = store.bucket_dir("b").join(DiskS3Store::subresource_filename(v));
            assert!(file.exists(), "{:?}", v);
            assert_eq!(std::fs::read_to_string(&file).unwrap(), "payload=true");
        }
    }

    #[test]
    fn delete_bucket_subresource_removes_file() {
        let tmp = TempDir::new().unwrap();
        let store = new_store(&tmp);
        store
            .put_bucket_meta("b", &BucketMeta {
                name: "b".to_string(),
                ..Default::default()
            })
            .unwrap();
        store
            .put_bucket_subresource("b", BucketSubresource::Tags, "x=1")
            .unwrap();
        store
            .delete_bucket_subresource("b", BucketSubresource::Tags)
            .unwrap();
        let file = store.bucket_dir("b").join("tags.toml");
        assert!(!file.exists());
        // idempotent
        store
            .delete_bucket_subresource("b", BucketSubresource::Tags)
            .unwrap();
    }

    #[test]
    fn put_object_bytes_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let store = new_store(&tmp);
        store
            .put_bucket_meta("b", &BucketMeta {
                name: "b".to_string(),
                ..Default::default()
            })
            .unwrap();
        let data = Bytes::from_static(b"hello world");
        let meta = sample_meta("k1", data.len() as u64);
        let body_ref = store
            .put_object("b", "k1", None, BodySource::Bytes(data.clone()), &meta)
            .unwrap();
        match &body_ref {
            BodyRef::Disk { bucket, key, size, path, .. } => {
                assert_eq!(bucket, "b");
                assert_eq!(key, "k1");
                assert_eq!(*size, data.len() as u64);
                assert_eq!(std::fs::read(path).unwrap(), data.to_vec());
            }
            _ => panic!("expected Disk"),
        }
        let loaded = store.load().unwrap();
        let snap = loaded.buckets.get("b").unwrap();
        let obj = snap.objects.get("k1").unwrap();
        assert_eq!(obj.meta.size, data.len() as u64);
    }

    #[test]
    fn put_object_file_source() {
        let tmp = TempDir::new().unwrap();
        let store = new_store(&tmp);
        store
            .put_bucket_meta("b", &BucketMeta {
                name: "b".to_string(),
                ..Default::default()
            })
            .unwrap();
        let src = tmp.path().join("src.bin");
        std::fs::write(&src, b"file-body").unwrap();
        let meta = sample_meta("k", 9);
        let body_ref = store
            .put_object("b", "k", None, BodySource::File(src.clone()), &meta)
            .unwrap();
        let path = match body_ref {
            BodyRef::Disk { path, .. } => path,
            _ => panic!(),
        };
        assert_eq!(std::fs::read(&path).unwrap(), b"file-body");
    }

    #[test]
    fn put_object_meta_only_keeps_bin() {
        let tmp = TempDir::new().unwrap();
        let store = new_store(&tmp);
        store
            .put_bucket_meta("b", &BucketMeta {
                name: "b".to_string(),
                ..Default::default()
            })
            .unwrap();
        let data = Bytes::from_static(b"abc");
        let mut meta = sample_meta("k", 3);
        store
            .put_object("b", "k", None, BodySource::Bytes(data.clone()), &meta)
            .unwrap();
        let (_, bin, _) = store.object_paths("b", "k", None);
        let before = std::fs::read(&bin).unwrap();
        meta.tags.insert("x".to_string(), "y".to_string());
        store.put_object_meta("b", "k", None, &meta).unwrap();
        assert_eq!(std::fs::read(&bin).unwrap(), before);
        let loaded = store.load().unwrap();
        let obj = loaded.buckets.get("b").unwrap().objects.get("k").unwrap();
        assert_eq!(obj.meta.tags.get("x").map(String::as_str), Some("y"));
    }

    #[test]
    fn delete_object_cleans_up_files_and_cache() {
        let tmp = TempDir::new().unwrap();
        let (store, cache) = new_store_with_cache(&tmp, 1024 * 1024);
        store
            .put_bucket_meta("b", &BucketMeta {
                name: "b".to_string(),
                ..Default::default()
            })
            .unwrap();
        let data = Bytes::from_static(b"bye");
        let meta = sample_meta("k", 3);
        store
            .put_object("b", "k", None, BodySource::Bytes(data), &meta)
            .unwrap();
        let body_key = crate::cache::BodyKey::new("b".to_string(), "k".to_string(), None);
        assert!(cache.get(&body_key).is_some());
        store.delete_object("b", "k", None).unwrap();
        let (dir, bin, toml_path) = store.object_paths("b", "k", None);
        assert!(!bin.exists());
        assert!(!toml_path.exists());
        assert!(!dir.exists());
        assert!(cache.get(&body_key).is_none());
    }

    #[test]
    fn open_object_body_cache_hit_and_refill() {
        let tmp = TempDir::new().unwrap();
        let (store, cache) = new_store_with_cache(&tmp, 1024 * 1024);
        store
            .put_bucket_meta("b", &BucketMeta {
                name: "b".to_string(),
                ..Default::default()
            })
            .unwrap();
        let data = Bytes::from_static(b"payload");
        let meta = sample_meta("k", data.len() as u64);
        let body_ref = store
            .put_object("b", "k", None, BodySource::Bytes(data.clone()), &meta)
            .unwrap();
        // Cache hit.
        let got = store.open_object_body(&body_ref).unwrap();
        assert_eq!(got, data);
        // Invalidate and re-read populates cache from disk.
        let body_key = crate::cache::BodyKey::new("b".to_string(), "k".to_string(), None);
        cache.invalidate(&body_key);
        assert!(cache.get(&body_key).is_none());
        let got = store.open_object_body(&body_ref).unwrap();
        assert_eq!(got, data);
        assert!(cache.get(&body_key).is_some());
    }

    #[test]
    fn open_object_body_large_bypasses_cache() {
        let tmp = TempDir::new().unwrap();
        // capacity 1024 → single-object cap 512. Use 800-byte body.
        let (store, cache) = new_store_with_cache(&tmp, 1024);
        store
            .put_bucket_meta("b", &BucketMeta {
                name: "b".to_string(),
                ..Default::default()
            })
            .unwrap();
        let data = Bytes::from(vec![7u8; 800]);
        let meta = sample_meta("big", 800);
        let body_ref = store
            .put_object("b", "big", None, BodySource::Bytes(data.clone()), &meta)
            .unwrap();
        let body_key = crate::cache::BodyKey::new("b".to_string(), "big".to_string(), None);
        assert!(cache.get(&body_key).is_none());
        let got = store.open_object_body(&body_ref).unwrap();
        assert_eq!(got, data);
        // Still none — exceeds single-object cap.
        assert!(cache.get(&body_key).is_none());
    }

    #[test]
    fn load_empty_dir() {
        let tmp = TempDir::new().unwrap();
        let store = new_store(&tmp);
        let s = store.load().unwrap();
        assert!(s.buckets.is_empty());
    }

    #[test]
    fn load_ignores_mpu_dir() {
        let tmp = TempDir::new().unwrap();
        let store = new_store(&tmp);
        store
            .put_bucket_meta("b", &BucketMeta {
                name: "b".to_string(),
                ..Default::default()
            })
            .unwrap();
        let data = Bytes::from_static(b"x");
        let meta = sample_meta("k", 1);
        store
            .put_object("b", "k", None, BodySource::Bytes(data), &meta)
            .unwrap();
        // TODO(phase-6): mpu dir is ignored by load.
        let mpu = store.bucket_dir("b").join("mpu").join("upload1");
        std::fs::create_dir_all(&mpu).unwrap();
        std::fs::write(mpu.join("init.toml"), "x").unwrap();

        let loaded = store.load().unwrap();
        let snap = loaded.buckets.get("b").unwrap();
        assert_eq!(snap.objects.len(), 1);
    }

    #[test]
    fn load_reads_bucket_subresources() {
        let tmp = TempDir::new().unwrap();
        let store = new_store(&tmp);
        store
            .put_bucket_meta("b", &BucketMeta {
                name: "b".to_string(),
                ..Default::default()
            })
            .unwrap();
        store
            .put_bucket_subresource("b", BucketSubresource::Lifecycle, "<Lifecycle/>")
            .unwrap();
        store
            .put_bucket_subresource("b", BucketSubresource::Cors, "<Cors/>")
            .unwrap();
        store
            .put_bucket_subresource("b", BucketSubresource::Policy, "{}")
            .unwrap();
        store
            .put_bucket_subresource("b", BucketSubresource::Tags, "x=1")
            .unwrap();
        let loaded = store.load().unwrap();
        let snap = loaded.buckets.get("b").unwrap();
        assert_eq!(
            snap.subresources.get("lifecycle.toml").map(String::as_str),
            Some("<Lifecycle/>"),
        );
        assert_eq!(
            snap.subresources.get("cors.toml").map(String::as_str),
            Some("<Cors/>"),
        );
        assert_eq!(
            snap.subresources.get("policy.toml").map(String::as_str),
            Some("{}"),
        );
        assert_eq!(
            snap.subresources.get("tags.toml").map(String::as_str),
            Some("x=1"),
        );
    }

    #[test]
    fn versioned_put_load_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let store = new_store(&tmp);
        store
            .put_bucket_meta("b", &BucketMeta {
                name: "b".to_string(),
                versioning: Some("Enabled".to_string()),
                ..Default::default()
            })
            .unwrap();
        let base = chrono::Utc::now();
        for (i, (vid, body)) in [("v1", "one"), ("v2", "two"), ("v3", "three")]
            .iter()
            .enumerate()
        {
            let mut m = sample_meta("k", body.len() as u64);
            m.version_id = Some((*vid).to_string());
            m.last_modified = base + chrono::Duration::seconds(i as i64);
            store
                .put_object(
                    "b",
                    "k",
                    Some(*vid),
                    BodySource::Bytes(Bytes::copy_from_slice(body.as_bytes())),
                    &m,
                )
                .unwrap();
        }
        let loaded = store.load().unwrap();
        let snap = loaded.buckets.get("b").unwrap();
        let versions = snap.object_versions.get("k").expect("versions present");
        assert_eq!(versions.len(), 3);
        assert_eq!(versions[0].meta.version_id.as_deref(), Some("v1"));
        assert_eq!(versions[1].meta.version_id.as_deref(), Some("v2"));
        assert_eq!(versions[2].meta.version_id.as_deref(), Some("v3"));
        for v in versions {
            match &v.body {
                BodyRef::Disk { size, .. } => assert!(*size > 0),
                _ => panic!("expected Disk body"),
            }
        }
    }

    #[test]
    fn delete_marker_roundtrip_no_body_file() {
        let tmp = TempDir::new().unwrap();
        let store = new_store(&tmp);
        store
            .put_bucket_meta("b", &BucketMeta {
                name: "b".to_string(),
                versioning: Some("Enabled".to_string()),
                ..Default::default()
            })
            .unwrap();
        let mut m = sample_meta("k", 0);
        m.version_id = Some("dm1".to_string());
        m.is_delete_marker = true;
        store
            .put_object("b", "k", Some("dm1"), BodySource::Bytes(Bytes::new()), &m)
            .unwrap();
        // No .bin file on disk for delete markers.
        let (_, bin, toml_path) = store.object_paths("b", "k", Some("dm1"));
        assert!(!bin.exists(), "delete marker must not have a .bin file");
        assert!(toml_path.exists());

        let loaded = store.load().unwrap();
        let versions = loaded
            .buckets
            .get("b")
            .unwrap()
            .object_versions
            .get("k")
            .unwrap();
        assert_eq!(versions.len(), 1);
        assert!(versions[0].meta.is_delete_marker);
        match &versions[0].body {
            BodyRef::Memory(b) => assert_eq!(b.len(), 0),
            _ => panic!("delete marker body should be empty Memory"),
        }
    }

    #[test]
    fn mixed_nonversioned_and_versioned_buckets() {
        let tmp = TempDir::new().unwrap();
        let store = new_store(&tmp);
        store
            .put_bucket_meta("a", &BucketMeta {
                name: "a".to_string(),
                ..Default::default()
            })
            .unwrap();
        store
            .put_bucket_meta("b", &BucketMeta {
                name: "b".to_string(),
                versioning: Some("Enabled".to_string()),
                ..Default::default()
            })
            .unwrap();
        let ma = sample_meta("only", 3);
        store
            .put_object("a", "only", None, BodySource::Bytes(Bytes::from_static(b"aaa")), &ma)
            .unwrap();
        let base = chrono::Utc::now();
        for (i, vid) in ["v1", "v2"].iter().enumerate() {
            let mut m = sample_meta("twice", 2);
            m.version_id = Some((*vid).to_string());
            m.last_modified = base + chrono::Duration::seconds(i as i64);
            store
                .put_object(
                    "b",
                    "twice",
                    Some(*vid),
                    BodySource::Bytes(Bytes::from_static(b"xx")),
                    &m,
                )
                .unwrap();
        }
        let loaded = store.load().unwrap();
        assert_eq!(loaded.buckets.len(), 2);
        let a = loaded.buckets.get("a").unwrap();
        assert_eq!(a.objects.len(), 1);
        assert!(a.object_versions.is_empty());
        let b = loaded.buckets.get("b").unwrap();
        assert!(b.objects.is_empty());
        assert_eq!(b.object_versions.get("twice").unwrap().len(), 2);
    }

    #[test]
    fn legacy_versioning_file_is_read() {
        let tmp = TempDir::new().unwrap();
        let store = new_store(&tmp);
        // Bucket meta with no versioning field set.
        store
            .put_bucket_meta("b", &BucketMeta {
                name: "b".to_string(),
                ..Default::default()
            })
            .unwrap();
        // Legacy sidecar: a bare versioning.toml with "Enabled".
        let path = store.bucket_dir("b").join("versioning.toml");
        std::fs::write(&path, "Enabled").unwrap();
        let loaded = store.load().unwrap();
        let snap = loaded.buckets.get("b").unwrap();
        assert_eq!(snap.meta.versioning.as_deref(), Some("Enabled"));
    }
}
