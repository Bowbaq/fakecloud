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
    pub objects: HashMap<String, ObjectMeta>,
    #[serde(default)]
    pub object_versions: HashMap<String, Vec<ObjectMeta>>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BodyRef {
    #[serde(skip)]
    Memory(Bytes),
    Disk {
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
    #[error("not supported by this store")]
    NotSupported,
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

// TODO(phase-4): full DiskS3Store implementation backed by the on-disk layout +
// BodyCache, including versioning, delete markers, and resumable multipart.
pub struct DiskS3Store {
    #[allow(dead_code)]
    root: PathBuf,
    #[allow(dead_code)]
    cache: crate::cache::BodyCache,
}

impl DiskS3Store {
    pub fn new(root: PathBuf, cache: crate::cache::BodyCache) -> Self {
        Self { root, cache }
    }
}

impl S3Store for DiskS3Store {
    fn load(&self) -> StoreResult<S3State> {
        todo!("DiskS3Store::load — phase 4")
    }
    fn put_bucket_meta(&self, _bucket: &str, _meta: &BucketMeta) -> StoreResult<()> {
        todo!("DiskS3Store — phase 4")
    }
    fn put_bucket_subresource(
        &self,
        _bucket: &str,
        _kind: BucketSubresource,
        _payload: &str,
    ) -> StoreResult<()> {
        todo!("DiskS3Store — phase 4")
    }
    fn delete_bucket_subresource(
        &self,
        _bucket: &str,
        _kind: BucketSubresource,
    ) -> StoreResult<()> {
        todo!("DiskS3Store — phase 4")
    }
    fn delete_bucket(&self, _bucket: &str) -> StoreResult<()> {
        todo!("DiskS3Store — phase 4")
    }
    fn put_object(
        &self,
        _bucket: &str,
        _key: &str,
        _version: Option<&str>,
        _body: BodySource,
        _meta: &ObjectMeta,
    ) -> StoreResult<BodyRef> {
        todo!("DiskS3Store — phase 4")
    }
    fn put_object_meta(
        &self,
        _bucket: &str,
        _key: &str,
        _version: Option<&str>,
        _meta: &ObjectMeta,
    ) -> StoreResult<()> {
        todo!("DiskS3Store — phase 4")
    }
    fn delete_object(
        &self,
        _bucket: &str,
        _key: &str,
        _version: Option<&str>,
    ) -> StoreResult<()> {
        todo!("DiskS3Store — phase 4")
    }
    fn open_object_body(&self, _body: &BodyRef) -> StoreResult<Bytes> {
        todo!("DiskS3Store — phase 4")
    }
    fn mpu_create(&self, _bucket: &str, _upload_id: &str, _init: &MpuInit) -> StoreResult<()> {
        todo!("DiskS3Store — phase 4")
    }
    fn mpu_put_part(
        &self,
        _bucket: &str,
        _upload_id: &str,
        _part_number: u32,
        _body: BodySource,
        _etag: &str,
    ) -> StoreResult<()> {
        todo!("DiskS3Store — phase 4")
    }
    fn mpu_abort(&self, _bucket: &str, _upload_id: &str) -> StoreResult<()> {
        todo!("DiskS3Store — phase 4")
    }
    fn mpu_complete(
        &self,
        _bucket: &str,
        _upload_id: &str,
        _final_key: &str,
        _version: Option<&str>,
        _meta: &ObjectMeta,
    ) -> StoreResult<BodyRef> {
        todo!("DiskS3Store — phase 4")
    }
}
