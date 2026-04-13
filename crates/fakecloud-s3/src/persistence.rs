use fakecloud_persistence::{
    AclGrantSnapshot, BucketMeta, MpuInit, ObjectMeta, UploadPartMeta,
};

use crate::state::{AclGrant, MultipartUpload, S3Bucket, S3Object, UploadPart};

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
