use std::collections::{BTreeMap, HashMap};

use fakecloud_persistence::{
    AclGrantSnapshot, BucketMeta, BucketSnapshot, LoadedObject, MpuInit, ObjectMeta,
    S3StateSnapshot, UploadPartMeta,
};

use crate::state::{AclGrant, MultipartUpload, S3Bucket, S3Object, S3State, UploadPart};

impl From<&AclGrant> for AclGrantSnapshot {
    fn from(g: &AclGrant) -> Self {
        Self {
            grantee_type: g.grantee_type.clone(),
            grantee_id: g.grantee_id.clone(),
            grantee_display_name: g.grantee_display_name.clone(),
            grantee_uri: g.grantee_uri.clone(),
            permission: g.permission.clone(),
        }
    }
}

pub fn bucket_meta_snapshot(b: &S3Bucket) -> BucketMeta {
    BucketMeta {
        name: b.name.clone(),
        creation_date: b.creation_date,
        region: b.region.clone(),
        versioning: b.versioning.clone(),
        acl: b.acl.clone(),
        acl_owner_id: b.acl_owner_id.clone(),
        accelerate_status: b.accelerate_status.clone(),
        eventbridge_enabled: b.eventbridge_enabled,
    }
}

pub fn object_meta_snapshot(o: &S3Object) -> ObjectMeta {
    ObjectMeta {
        key: o.key.clone(),
        content_type: o.content_type.clone(),
        etag: o.etag.clone(),
        size: o.size,
        last_modified: o.last_modified,
        metadata: o.metadata.clone(),
        tags: o.tags.clone(),
        storage_class: o.storage_class.clone(),
        acl_grants: o.acl_grants.iter().map(AclGrantSnapshot::from).collect(),
        acl_owner_id: o.acl_owner_id.clone(),
        parts_count: o.parts_count,
        part_sizes: o.part_sizes.clone(),
        sse_algorithm: o.sse_algorithm.clone(),
        sse_kms_key_id: o.sse_kms_key_id.clone(),
        bucket_key_enabled: o.bucket_key_enabled,
        version_id: o.version_id.clone(),
        is_delete_marker: o.is_delete_marker,
        restore_ongoing: o.restore_ongoing,
        restore_expiry: o.restore_expiry.clone(),
        checksum_algorithm: o.checksum_algorithm.clone(),
        checksum_value: o.checksum_value.clone(),
        lock_mode: o.lock_mode.clone(),
        lock_retain_until: o.lock_retain_until,
        lock_legal_hold: o.lock_legal_hold.clone(),
        content_encoding: o.content_encoding.clone(),
        website_redirect_location: o.website_redirect_location.clone(),
    }
}

pub fn mpu_init_snapshot(m: &MultipartUpload) -> MpuInit {
    MpuInit {
        upload_id: m.upload_id.clone(),
        key: m.key.clone(),
        initiated: m.initiated,
        metadata: m.metadata.clone(),
        content_type: m.content_type.clone(),
        storage_class: m.storage_class.clone(),
        sse_algorithm: m.sse_algorithm.clone(),
        sse_kms_key_id: m.sse_kms_key_id.clone(),
        tagging: m.tagging.clone(),
        acl_grants: m.acl_grants.iter().map(AclGrantSnapshot::from).collect(),
        checksum_algorithm: m.checksum_algorithm.clone(),
    }
}

pub fn upload_part_meta_snapshot(p: &UploadPart) -> UploadPartMeta {
    UploadPartMeta {
        part_number: p.part_number,
        etag: p.etag.clone(),
        size: p.size,
        last_modified: p.last_modified,
    }
}

fn acl_grant_from_snapshot(g: &AclGrantSnapshot) -> AclGrant {
    AclGrant {
        grantee_type: g.grantee_type.clone(),
        grantee_id: g.grantee_id.clone(),
        grantee_display_name: g.grantee_display_name.clone(),
        grantee_uri: g.grantee_uri.clone(),
        permission: g.permission.clone(),
    }
}

pub fn s3_object_from_loaded(lo: LoadedObject) -> S3Object {
    let LoadedObject { meta, body } = lo;
    S3Object {
        key: meta.key,
        body,
        content_type: meta.content_type,
        etag: meta.etag,
        size: meta.size,
        last_modified: meta.last_modified,
        metadata: meta.metadata,
        storage_class: meta.storage_class,
        tags: meta.tags,
        acl_grants: meta.acl_grants.iter().map(acl_grant_from_snapshot).collect(),
        acl_owner_id: meta.acl_owner_id,
        parts_count: meta.parts_count,
        part_sizes: meta.part_sizes,
        sse_algorithm: meta.sse_algorithm,
        sse_kms_key_id: meta.sse_kms_key_id,
        bucket_key_enabled: meta.bucket_key_enabled,
        version_id: meta.version_id,
        is_delete_marker: meta.is_delete_marker,
        content_encoding: meta.content_encoding,
        website_redirect_location: meta.website_redirect_location,
        restore_ongoing: meta.restore_ongoing,
        restore_expiry: meta.restore_expiry,
        checksum_algorithm: meta.checksum_algorithm,
        checksum_value: meta.checksum_value,
        lock_mode: meta.lock_mode,
        lock_retain_until: meta.lock_retain_until,
        lock_legal_hold: meta.lock_legal_hold,
    }
}

pub fn s3_bucket_from_snapshot(name: &str, snap: BucketSnapshot, default_region: &str) -> S3Bucket {
    let BucketSnapshot {
        meta,
        objects,
        object_versions,
        subresources,
    } = snap;
    let region = if meta.region.is_empty() {
        default_region.to_string()
    } else {
        meta.region.clone()
    };
    let mut b = S3Bucket {
        name: name.to_string(),
        creation_date: meta.creation_date,
        region,
        objects: BTreeMap::new(),
        tags: HashMap::new(),
        acl_grants: Vec::new(),
        acl_owner_id: meta.acl_owner_id.clone(),
        multipart_uploads: HashMap::new(),
        versioning: meta.versioning.clone(),
        object_versions: HashMap::new(),
        acl: meta.acl.clone(),
        encryption_config: None,
        lifecycle_config: None,
        policy: None,
        cors_config: None,
        notification_config: None,
        logging_config: None,
        website_config: None,
        accelerate_status: meta.accelerate_status.clone(),
        public_access_block: None,
        object_lock_config: None,
        replication_config: None,
        ownership_controls: None,
        inventory_configs: HashMap::new(),
        eventbridge_enabled: meta.eventbridge_enabled,
    };
    for (key, lo) in objects {
        b.objects.insert(key, s3_object_from_loaded(lo));
    }
    for (key, vs) in object_versions {
        b.object_versions
            .insert(key, vs.into_iter().map(s3_object_from_loaded).collect());
    }
    // Subresources are stored as raw pass-through strings on the service side;
    // fold any present into the matching Option<String> field.
    for (fname, text) in subresources {
        match fname.as_str() {
            "lifecycle.toml" => b.lifecycle_config = Some(text),
            "cors.toml" => b.cors_config = Some(text),
            "policy.toml" => b.policy = Some(text),
            "notification.toml" => b.notification_config = Some(text),
            "logging.toml" => b.logging_config = Some(text),
            "website.toml" => b.website_config = Some(text),
            "public_access_block.toml" => b.public_access_block = Some(text),
            "object_lock.toml" => b.object_lock_config = Some(text),
            "replication.toml" => b.replication_config = Some(text),
            "ownership.toml" => b.ownership_controls = Some(text),
            "encryption.toml" => b.encryption_config = Some(text),
            // tags.toml, acl.toml, inventory.toml, accelerate.toml, versioning.toml
            // are covered by BucketMeta (versioning/accelerate) or are pre-existing
            // phase-2 holes (tags, acl_grants, inventory) with empty payloads today.
            _ => {}
        }
    }
    b
}

pub fn hydrate_s3_state(
    snapshot: S3StateSnapshot,
    account_id: &str,
    region: &str,
) -> S3State {
    let mut state = S3State::new(account_id, region);
    for (name, snap) in snapshot.buckets {
        let bucket = s3_bucket_from_snapshot(&name, snap, region);
        state.buckets.insert(name, bucket);
    }
    state
}
