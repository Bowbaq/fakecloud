use async_trait::async_trait;
use bytes::Bytes;
use chrono::{DateTime, Timelike, Utc};
use http::{HeaderMap, Method, StatusCode};
use md5::{Digest, Md5};

use fakecloud_core::service::{AwsRequest, AwsResponse, AwsService, AwsServiceError};

use crate::state::{AclGrant, MultipartUpload, S3Bucket, S3Object, SharedS3State, UploadPart};

pub struct S3Service {
    state: SharedS3State,
}

impl S3Service {
    pub fn new(state: SharedS3State) -> Self {
        Self { state }
    }
}

#[async_trait]
impl AwsService for S3Service {
    fn service_name(&self) -> &str {
        "s3"
    }

    async fn handle(&self, req: AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        // S3 REST routing: method + path segments + query params
        let bucket = req.path_segments.first().map(|s| s.as_str());
        let key = if req.path_segments.len() > 1 {
            Some(req.path_segments[1..].join("/"))
        } else {
            None
        };

        // Multipart upload operations (checked before main match)
        if let Some(b) = bucket {
            // POST /{bucket}/{key}?uploads — CreateMultipartUpload
            if req.method == Method::POST
                && key.is_some()
                && req.query_params.contains_key("uploads")
            {
                return self.create_multipart_upload(&req, b, key.as_deref().unwrap());
            }

            // POST /{bucket}/{key}?uploadId=X — CompleteMultipartUpload
            if req.method == Method::POST && key.is_some() {
                if let Some(upload_id) = req.query_params.get("uploadId").cloned() {
                    return self.complete_multipart_upload(
                        &req,
                        b,
                        key.as_deref().unwrap(),
                        &upload_id,
                    );
                }
            }

            // PUT /{bucket}/{key}?partNumber=N&uploadId=X — UploadPart or UploadPartCopy
            if req.method == Method::PUT && key.is_some() {
                if let (Some(part_num_str), Some(upload_id)) = (
                    req.query_params.get("partNumber").cloned(),
                    req.query_params.get("uploadId").cloned(),
                ) {
                    if let Ok(part_number) = part_num_str.parse::<i64>() {
                        if req.headers.contains_key("x-amz-copy-source") {
                            return self.upload_part_copy(
                                &req,
                                b,
                                key.as_deref().unwrap(),
                                &upload_id,
                                part_number,
                            );
                        }
                        return self.upload_part(
                            &req,
                            b,
                            key.as_deref().unwrap(),
                            &upload_id,
                            part_number,
                        );
                    }
                }
            }

            // DELETE /{bucket}/{key}?uploadId=X — AbortMultipartUpload
            if req.method == Method::DELETE && key.is_some() {
                if let Some(upload_id) = req.query_params.get("uploadId").cloned() {
                    return self.abort_multipart_upload(b, key.as_deref().unwrap(), &upload_id);
                }
            }

            // GET /{bucket}?uploads — ListMultipartUploads
            if req.method == Method::GET
                && key.is_none()
                && req.query_params.contains_key("uploads")
            {
                return self.list_multipart_uploads(b);
            }

            // GET /{bucket}/{key}?uploadId=X — ListParts
            if req.method == Method::GET && key.is_some() {
                if let Some(upload_id) = req.query_params.get("uploadId").cloned() {
                    return self.list_parts(&req, b, key.as_deref().unwrap(), &upload_id);
                }
            }
        }

        match (&req.method, bucket, key.as_deref()) {
            // ListBuckets: GET /
            (&Method::GET, None, None) => self.list_buckets(&req),

            // Bucket-level operations (no key)
            (&Method::PUT, Some(b), None) => {
                if req.query_params.contains_key("tagging") {
                    self.put_bucket_tagging(&req, b)
                } else if req.query_params.contains_key("acl") {
                    self.put_bucket_acl(&req, b)
                } else if req.query_params.contains_key("versioning") {
                    self.put_bucket_versioning(&req, b)
                } else {
                    self.create_bucket(&req, b)
                }
            }
            (&Method::DELETE, Some(b), None) => {
                if req.query_params.contains_key("tagging") {
                    self.delete_bucket_tagging(&req, b)
                } else {
                    self.delete_bucket(&req, b)
                }
            }
            (&Method::HEAD, Some(b), None) => self.head_bucket(b),
            (&Method::GET, Some(b), None) => {
                if req.query_params.contains_key("tagging") {
                    self.get_bucket_tagging(&req, b)
                } else if req.query_params.contains_key("location") {
                    self.get_bucket_location(b)
                } else if req.query_params.contains_key("acl") {
                    self.get_bucket_acl(&req, b)
                } else if req.query_params.contains_key("versioning") {
                    self.get_bucket_versioning(b)
                } else if req.query_params.contains_key("versions") {
                    self.list_object_versions(b)
                } else if req.query_params.contains_key("object-lock") {
                    self.get_object_lock_configuration(b)
                } else {
                    self.list_objects_v2(&req, b)
                }
            }

            // Object-level operations
            (&Method::PUT, Some(b), Some(k)) => {
                if req.query_params.contains_key("tagging") {
                    self.put_object_tagging(&req, b, k)
                } else if req.query_params.contains_key("acl") {
                    self.put_object_acl(&req, b, k)
                } else if req.headers.contains_key("x-amz-copy-source") {
                    self.copy_object(&req, b, k)
                } else {
                    self.put_object(&req, b, k)
                }
            }
            (&Method::GET, Some(b), Some(k)) => {
                if req.query_params.contains_key("tagging") {
                    self.get_object_tagging(&req, b, k)
                } else if req.query_params.contains_key("acl") {
                    self.get_object_acl(&req, b, k)
                } else {
                    self.get_object(&req, b, k)
                }
            }
            (&Method::DELETE, Some(b), Some(k)) => {
                if req.query_params.contains_key("tagging") {
                    self.delete_object_tagging(b, k)
                } else {
                    self.delete_object(&req, b, k)
                }
            }
            (&Method::HEAD, Some(b), Some(k)) => self.head_object(&req, b, k),

            // POST /{bucket}?delete — batch delete
            (&Method::POST, Some(b), None) if req.query_params.contains_key("delete") => {
                self.delete_objects(&req, b)
            }

            _ => Err(AwsServiceError::aws_error(
                StatusCode::METHOD_NOT_ALLOWED,
                "MethodNotAllowed",
                "The specified method is not allowed against this resource",
            )),
        }
    }

    fn supported_actions(&self) -> &[&str] {
        &[
            "ListBuckets",
            "CreateBucket",
            "DeleteBucket",
            "HeadBucket",
            "ListObjectsV2",
            "PutObject",
            "GetObject",
            "DeleteObject",
            "HeadObject",
            "CopyObject",
            "DeleteObjects",
            "GetBucketLocation",
            "GetBucketTagging",
            "PutBucketTagging",
            "DeleteBucketTagging",
            "GetBucketAcl",
            "PutBucketAcl",
            "GetObjectAcl",
            "PutObjectAcl",
            "GetObjectTagging",
            "PutObjectTagging",
            "DeleteObjectTagging",
            "PutBucketVersioning",
            "GetBucketVersioning",
        ]
    }
}

// ---------------------------------------------------------------------------
// Bucket operations
// ---------------------------------------------------------------------------
impl S3Service {
    fn list_buckets(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let state = self.state.read();
        let mut buckets_xml = String::new();
        let mut sorted: Vec<_> = state.buckets.values().collect();
        sorted.sort_by_key(|b| &b.name);
        for b in sorted {
            buckets_xml.push_str(&format!(
                "<Bucket><Name>{}</Name><CreationDate>{}</CreationDate></Bucket>",
                xml_escape(&b.name),
                b.creation_date.format("%Y-%m-%dT%H:%M:%S%.3fZ"),
            ));
        }
        let body = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
             <ListAllMyBucketsResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\
             <Owner><ID>{account}</ID><DisplayName>{account}</DisplayName></Owner>\
             <Buckets>{buckets_xml}</Buckets>\
             </ListAllMyBucketsResult>",
            account = req.account_id,
        );
        Ok(AwsResponse::xml(StatusCode::OK, body))
    }

    fn create_bucket(
        &self,
        req: &AwsRequest,
        bucket: &str,
    ) -> Result<AwsResponse, AwsServiceError> {
        if !is_valid_bucket_name(bucket) {
            return Err(AwsServiceError::aws_error(
                StatusCode::BAD_REQUEST,
                "InvalidBucketName",
                format!("The specified bucket is not valid: {bucket}"),
            ));
        }

        // Parse LocationConstraint from body if present
        let body_str = std::str::from_utf8(&req.body).unwrap_or("");
        if !body_str.is_empty() {
            if let Some(constraint) = extract_xml_value(body_str, "LocationConstraint") {
                if !constraint.is_empty() && !is_valid_region(&constraint) {
                    return Err(AwsServiceError::aws_error(
                        StatusCode::BAD_REQUEST,
                        "InvalidLocationConstraint",
                        format!("The specified location-constraint is not valid: {constraint}"),
                    ));
                }
            }
        }

        // Parse ACL from header
        let acl = req
            .headers
            .get("x-amz-acl")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("private");

        let mut state = self.state.write();
        if state.buckets.contains_key(bucket) {
            return Err(AwsServiceError::aws_error(
                StatusCode::CONFLICT,
                "BucketAlreadyOwnedByYou",
                "Your previous request to create the named bucket succeeded and you already own it.",
            ));
        }
        let mut b = S3Bucket::new(bucket, &req.region, &req.account_id);
        b.acl_grants = canned_acl_grants(acl, &req.account_id);
        state.buckets.insert(bucket.to_string(), b);

        let mut headers = HeaderMap::new();
        headers.insert("location", format!("/{bucket}").parse().unwrap());
        Ok(AwsResponse {
            status: StatusCode::OK,
            content_type: "application/xml".to_string(),
            body: Bytes::new(),
            headers,
        })
    }

    fn delete_bucket(
        &self,
        _req: &AwsRequest,
        bucket: &str,
    ) -> Result<AwsResponse, AwsServiceError> {
        let mut state = self.state.write();
        let b = state
            .buckets
            .get(bucket)
            .ok_or_else(|| no_such_bucket(bucket))?;
        if !b.objects.is_empty() {
            return Err(AwsServiceError::aws_error(
                StatusCode::CONFLICT,
                "BucketNotEmpty",
                "The bucket you tried to delete is not empty",
            ));
        }
        state.buckets.remove(bucket);
        Ok(AwsResponse {
            status: StatusCode::NO_CONTENT,
            content_type: "application/xml".to_string(),
            body: Bytes::new(),
            headers: HeaderMap::new(),
        })
    }

    fn head_bucket(&self, bucket: &str) -> Result<AwsResponse, AwsServiceError> {
        let state = self.state.read();
        if !state.buckets.contains_key(bucket) {
            return Err(AwsServiceError::aws_error(
                StatusCode::NOT_FOUND,
                "NoSuchBucket",
                format!("The specified bucket does not exist: {bucket}"),
            ));
        }
        Ok(AwsResponse {
            status: StatusCode::OK,
            content_type: "application/xml".to_string(),
            body: Bytes::new(),
            headers: HeaderMap::new(),
        })
    }

    fn get_bucket_location(&self, bucket: &str) -> Result<AwsResponse, AwsServiceError> {
        let state = self.state.read();
        let b = state
            .buckets
            .get(bucket)
            .ok_or_else(|| no_such_bucket(bucket))?;
        let loc = if b.region == "us-east-1" {
            String::new()
        } else {
            b.region.clone()
        };
        let body = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
             <LocationConstraint xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">{loc}</LocationConstraint>"
        );
        Ok(AwsResponse::xml(StatusCode::OK, body))
    }

    fn list_objects_v2(
        &self,
        req: &AwsRequest,
        bucket: &str,
    ) -> Result<AwsResponse, AwsServiceError> {
        let state = self.state.read();
        let b = state
            .buckets
            .get(bucket)
            .ok_or_else(|| no_such_bucket(bucket))?;

        let prefix = req.query_params.get("prefix").cloned().unwrap_or_default();
        let delimiter = req
            .query_params
            .get("delimiter")
            .cloned()
            .unwrap_or_default();
        let max_keys: usize = req
            .query_params
            .get("max-keys")
            .and_then(|v| v.parse().ok())
            .unwrap_or(1000);
        let start_after = req
            .query_params
            .get("start-after")
            .cloned()
            .unwrap_or_default();
        let continuation = req.query_params.get("continuation-token").cloned();

        let effective_start = continuation.as_deref().unwrap_or(&start_after);

        let mut contents = String::new();
        let mut common_prefixes: Vec<String> = Vec::new();
        let mut count = 0;
        let mut is_truncated = false;
        let mut last_key = String::new();

        for (key, obj) in &b.objects {
            if !key.starts_with(&prefix) {
                continue;
            }
            if !effective_start.is_empty() && key.as_str() <= effective_start {
                continue;
            }

            // Handle delimiter-based grouping
            if !delimiter.is_empty() {
                let suffix = &key[prefix.len()..];
                if let Some(pos) = suffix.find(&delimiter) {
                    let cp = format!("{}{}", prefix, &suffix[..=pos]);
                    if !common_prefixes.contains(&cp) {
                        if count >= max_keys {
                            is_truncated = true;
                            break;
                        }
                        common_prefixes.push(cp);
                        last_key = key.clone();
                        count += 1;
                    }
                    continue;
                }
            }

            if count >= max_keys {
                is_truncated = true;
                break;
            }

            contents.push_str(&format!(
                "<Contents>\
                 <Key>{}</Key>\
                 <LastModified>{}</LastModified>\
                 <ETag>&quot;{}&quot;</ETag>\
                 <Size>{}</Size>\
                 <StorageClass>{}</StorageClass>\
                 </Contents>",
                xml_escape(key),
                obj.last_modified.format("%Y-%m-%dT%H:%M:%S%.3fZ"),
                obj.etag,
                obj.size,
                obj.storage_class,
            ));
            last_key = key.clone();
            count += 1;
        }

        let mut common_prefixes_xml = String::new();
        for cp in &common_prefixes {
            common_prefixes_xml.push_str(&format!(
                "<CommonPrefixes><Prefix>{}</Prefix></CommonPrefixes>",
                xml_escape(cp),
            ));
        }

        let next_token = if is_truncated {
            format!(
                "<NextContinuationToken>{}</NextContinuationToken>",
                xml_escape(&last_key)
            )
        } else {
            String::new()
        };

        let cont_token = if let Some(ct) = &continuation {
            format!("<ContinuationToken>{}</ContinuationToken>", xml_escape(ct))
        } else {
            String::new()
        };

        let body = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
             <ListBucketResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\
             <Name>{bucket}</Name>\
             <Prefix>{prefix}</Prefix>\
             <KeyCount>{count}</KeyCount>\
             <MaxKeys>{max_keys}</MaxKeys>\
             <IsTruncated>{is_truncated}</IsTruncated>\
             {cont_token}\
             {next_token}\
             {contents}\
             {common_prefixes_xml}\
             </ListBucketResult>",
            prefix = xml_escape(&prefix),
        );
        Ok(AwsResponse::xml(StatusCode::OK, body))
    }

    fn get_bucket_tagging(
        &self,
        _req: &AwsRequest,
        bucket: &str,
    ) -> Result<AwsResponse, AwsServiceError> {
        let state = self.state.read();
        let b = state
            .buckets
            .get(bucket)
            .ok_or_else(|| no_such_bucket(bucket))?;
        if b.tags.is_empty() {
            return Err(AwsServiceError::aws_error_with_extra(
                StatusCode::NOT_FOUND,
                "NoSuchTagSet",
                "The TagSet does not exist",
                &[("BucketName", &b.name)],
            ));
        }
        let mut tags_xml = String::new();
        for (k, v) in &b.tags {
            tags_xml.push_str(&format!(
                "<Tag><Key>{}</Key><Value>{}</Value></Tag>",
                xml_escape(k),
                xml_escape(v),
            ));
        }
        let body = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
             <Tagging xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\
             <TagSet>{tags_xml}</TagSet></Tagging>"
        );
        Ok(AwsResponse::xml(StatusCode::OK, body))
    }

    fn put_bucket_tagging(
        &self,
        req: &AwsRequest,
        bucket: &str,
    ) -> Result<AwsResponse, AwsServiceError> {
        let body_str = std::str::from_utf8(&req.body).unwrap_or("");
        let tags = parse_tagging_xml(body_str);

        // Validate tags: no duplicate keys
        validate_tags(&tags)?;

        let mut state = self.state.write();
        let b = state
            .buckets
            .get_mut(bucket)
            .ok_or_else(|| no_such_bucket(bucket))?;
        b.tags = tags.into_iter().collect();
        Ok(AwsResponse {
            status: StatusCode::NO_CONTENT,
            content_type: "application/xml".to_string(),
            body: Bytes::new(),
            headers: HeaderMap::new(),
        })
    }

    fn delete_bucket_tagging(
        &self,
        _req: &AwsRequest,
        bucket: &str,
    ) -> Result<AwsResponse, AwsServiceError> {
        let mut state = self.state.write();
        let b = state
            .buckets
            .get_mut(bucket)
            .ok_or_else(|| no_such_bucket(bucket))?;
        b.tags.clear();
        Ok(AwsResponse {
            status: StatusCode::NO_CONTENT,
            content_type: "application/xml".to_string(),
            body: Bytes::new(),
            headers: HeaderMap::new(),
        })
    }

    // ---- Bucket ACL ----

    fn get_bucket_acl(
        &self,
        req: &AwsRequest,
        bucket: &str,
    ) -> Result<AwsResponse, AwsServiceError> {
        let state = self.state.read();
        let b = state
            .buckets
            .get(bucket)
            .ok_or_else(|| no_such_bucket(bucket))?;

        let body = build_acl_xml(&b.acl_owner_id, &b.acl_grants, &req.account_id);
        Ok(AwsResponse::xml(StatusCode::OK, body))
    }

    fn put_bucket_acl(
        &self,
        req: &AwsRequest,
        bucket: &str,
    ) -> Result<AwsResponse, AwsServiceError> {
        // Check for canned ACL header
        let canned = req
            .headers
            .get("x-amz-acl")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let mut state = self.state.write();
        let b = state
            .buckets
            .get_mut(bucket)
            .ok_or_else(|| no_such_bucket(bucket))?;

        if let Some(acl) = canned {
            b.acl_grants = canned_acl_grants(&acl, &b.acl_owner_id.clone());
        } else {
            // Parse ACL from body (AccessControlPolicy XML)
            let body_str = std::str::from_utf8(&req.body).unwrap_or("");
            let grants = parse_acl_xml(body_str)?;
            b.acl_grants = grants;
        }

        Ok(AwsResponse {
            status: StatusCode::OK,
            content_type: "application/xml".to_string(),
            body: Bytes::new(),
            headers: HeaderMap::new(),
        })
    }

    // ---- Bucket Versioning ----

    fn put_bucket_versioning(
        &self,
        req: &AwsRequest,
        bucket: &str,
    ) -> Result<AwsResponse, AwsServiceError> {
        let body_str = std::str::from_utf8(&req.body).unwrap_or("");
        let status_val = extract_xml_value(body_str, "Status").unwrap_or_default();

        let mut state = self.state.write();
        let b = state
            .buckets
            .get_mut(bucket)
            .ok_or_else(|| no_such_bucket(bucket))?;
        if status_val == "Enabled" || status_val == "Suspended" {
            b.versioning = Some(status_val);
        }
        Ok(AwsResponse {
            status: StatusCode::OK,
            content_type: "application/xml".to_string(),
            body: Bytes::new(),
            headers: HeaderMap::new(),
        })
    }

    fn get_bucket_versioning(&self, bucket: &str) -> Result<AwsResponse, AwsServiceError> {
        let state = self.state.read();
        let b = state
            .buckets
            .get(bucket)
            .ok_or_else(|| no_such_bucket(bucket))?;
        let status_xml = match &b.versioning {
            Some(s) => format!("<Status>{s}</Status>"),
            None => String::new(),
        };
        let body = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
             <VersioningConfiguration xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\
             {status_xml}\
             </VersioningConfiguration>"
        );
        Ok(AwsResponse::xml(StatusCode::OK, body))
    }

    fn list_object_versions(&self, bucket: &str) -> Result<AwsResponse, AwsServiceError> {
        let state = self.state.read();
        let b = state
            .buckets
            .get(bucket)
            .ok_or_else(|| no_such_bucket(bucket))?;

        // Without real versioning support, return current objects as "null" version
        let mut versions_xml = String::new();
        for (key, obj) in &b.objects {
            versions_xml.push_str(&format!(
                "<Version>\
                 <Key>{}</Key>\
                 <VersionId>null</VersionId>\
                 <IsLatest>true</IsLatest>\
                 <LastModified>{}</LastModified>\
                 <ETag>&quot;{}&quot;</ETag>\
                 <Size>{}</Size>\
                 <StorageClass>{}</StorageClass>\
                 </Version>",
                xml_escape(key),
                obj.last_modified.format("%Y-%m-%dT%H:%M:%S%.3fZ"),
                obj.etag,
                obj.size,
                obj.storage_class,
            ));
        }

        let body = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
             <ListVersionsResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\
             <Name>{}</Name>\
             <IsTruncated>false</IsTruncated>\
             {versions_xml}\
             </ListVersionsResult>",
            xml_escape(bucket),
        );
        Ok(AwsResponse::xml(StatusCode::OK, body))
    }

    fn get_object_lock_configuration(&self, bucket: &str) -> Result<AwsResponse, AwsServiceError> {
        let state = self.state.read();
        if !state.buckets.contains_key(bucket) {
            return Err(no_such_bucket(bucket));
        }
        // Object Lock is not configured
        Err(AwsServiceError::aws_error(
            StatusCode::NOT_FOUND,
            "ObjectLockConfigurationNotFoundError",
            "Object Lock configuration does not exist for this bucket",
        ))
    }
}

// ---------------------------------------------------------------------------
// Object operations
// ---------------------------------------------------------------------------
impl S3Service {
    fn put_object(
        &self,
        req: &AwsRequest,
        bucket: &str,
        key: &str,
    ) -> Result<AwsResponse, AwsServiceError> {
        // Validate key length
        if key.len() > 1024 {
            return Err(AwsServiceError::aws_error(
                StatusCode::BAD_REQUEST,
                "KeyTooLongError",
                "Your key is too long",
            ));
        }

        // Check for If-None-Match conditional on PUT
        let if_none_match = req
            .headers
            .get("if-none-match")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        // Check for If-Match conditional on PUT
        let if_match = req
            .headers
            .get("if-match")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        // Check for x-amz-tagging header
        let tagging_header = req
            .headers
            .get("x-amz-tagging")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        // Check for ACL header
        let acl_header = req
            .headers
            .get("x-amz-acl")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        // Check for grant headers alongside canned ACL
        let has_grant_headers = req.headers.keys().any(|k| {
            let name = k.as_str();
            name.starts_with("x-amz-grant-")
        });

        if acl_header.is_some() && has_grant_headers {
            return Err(AwsServiceError::aws_error(
                StatusCode::BAD_REQUEST,
                "InvalidRequest",
                "Specifying both Canned ACLs and Header Grants is not allowed",
            ));
        }

        // Parse tags from header
        let tags = if let Some(tagging) = &tagging_header {
            let parsed = parse_url_encoded_tags(tagging);
            // Validate aws: prefix
            for (k, _) in &parsed {
                if k.starts_with("aws:") {
                    return Err(AwsServiceError::aws_error(
                        StatusCode::BAD_REQUEST,
                        "InvalidTag",
                        "Your TagKey cannot be prefixed with aws:",
                    ));
                }
            }
            parsed.into_iter().collect()
        } else {
            std::collections::HashMap::new()
        };

        let mut state = self.state.write();
        let b = state
            .buckets
            .get_mut(bucket)
            .ok_or_else(|| no_such_bucket(bucket))?;

        // Handle If-Match: check existing object etag
        if let Some(ref if_match_val) = if_match {
            match b.objects.get(key) {
                Some(existing) => {
                    let existing_etag = format!("\"{}\"", existing.etag);
                    if !etag_matches(if_match_val, &existing_etag) {
                        return Err(precondition_failed("If-Match"));
                    }
                }
                None => {
                    return Err(no_such_key(key));
                }
            }
        }

        // Handle If-None-Match: if "*", fail if object already exists
        if let Some(ref inm) = if_none_match {
            if inm.trim() == "*" && b.objects.contains_key(key) {
                return Err(precondition_failed("If-None-Match"));
            }
        }

        let data = req.body.clone();
        let etag = compute_md5(&data);
        let content_type = req
            .headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();

        let metadata = extract_user_metadata(&req.headers);

        // Build ACL grants for object
        let acl_grants = if has_grant_headers {
            parse_grant_headers(&req.headers)
        } else if let Some(ref acl) = acl_header {
            canned_acl_grants_for_object(acl, &b.acl_owner_id)
        } else {
            // Default: owner full control
            vec![AclGrant {
                grantee_type: "CanonicalUser".to_string(),
                grantee_id: Some(b.acl_owner_id.clone()),
                grantee_display_name: Some(b.acl_owner_id.clone()),
                grantee_uri: None,
                permission: "FULL_CONTROL".to_string(),
            }]
        };

        let obj = S3Object {
            key: key.to_string(),
            size: data.len() as u64,
            data,
            content_type,
            etag: etag.clone(),
            last_modified: Utc::now(),
            metadata,
            storage_class: "STANDARD".to_string(),
            tags,
            acl_grants,
            acl_owner_id: Some(b.acl_owner_id.clone()),
            parts_count: None,
            part_sizes: None,
            sse_algorithm: None,
            sse_kms_key_id: None,
            bucket_key_enabled: None,
            website_redirect_location: None,
        };
        b.objects.insert(key.to_string(), obj);

        let mut headers = HeaderMap::new();
        headers.insert("etag", format!("\"{etag}\"").parse().unwrap());
        Ok(AwsResponse {
            status: StatusCode::OK,
            content_type: "application/xml".to_string(),
            body: Bytes::new(),
            headers,
        })
    }

    fn get_object(
        &self,
        req: &AwsRequest,
        bucket: &str,
        key: &str,
    ) -> Result<AwsResponse, AwsServiceError> {
        let state = self.state.read();
        let b = state
            .buckets
            .get(bucket)
            .ok_or_else(|| no_such_bucket(bucket))?;
        let obj = b.objects.get(key).ok_or_else(|| no_such_key(key))?;

        // Conditional checks
        check_get_conditionals(req, obj)?;

        let mut headers = HeaderMap::new();
        headers.insert("etag", format!("\"{}\"", obj.etag).parse().unwrap());
        headers.insert(
            "last-modified",
            obj.last_modified
                .format("%a, %d %b %Y %H:%M:%S GMT")
                .to_string()
                .parse()
                .unwrap(),
        );
        headers.insert("content-length", obj.size.to_string().parse().unwrap());
        for (k, v) in &obj.metadata {
            if let (Ok(name), Ok(val)) = (
                format!("x-amz-meta-{k}").parse::<http::header::HeaderName>(),
                v.parse::<http::header::HeaderValue>(),
            ) {
                headers.insert(name, val);
            }
        }

        // Add tag count if object has tags
        if !obj.tags.is_empty() {
            headers.insert(
                "x-amz-tagging-count",
                obj.tags.len().to_string().parse().unwrap(),
            );
        }

        if obj.storage_class != "STANDARD" {
            headers.insert("x-amz-storage-class", obj.storage_class.parse().unwrap());
        }

        if let Some(algo) = &obj.sse_algorithm {
            headers.insert("x-amz-server-side-encryption", algo.parse().unwrap());
        }
        if let Some(kid) = &obj.sse_kms_key_id {
            headers.insert(
                "x-amz-server-side-encryption-aws-kms-key-id",
                kid.parse().unwrap(),
            );
        }
        if let Some(true) = obj.bucket_key_enabled {
            headers.insert(
                "x-amz-server-side-encryption-bucket-key-enabled",
                "true".parse().unwrap(),
            );
        }
        if let Some(redirect) = &obj.website_redirect_location {
            headers.insert("x-amz-website-redirect-location", redirect.parse().unwrap());
        }

        Ok(AwsResponse {
            status: StatusCode::OK,
            content_type: obj.content_type.clone(),
            body: obj.data.clone(),
            headers,
        })
    }

    fn delete_object(
        &self,
        req: &AwsRequest,
        bucket: &str,
        key: &str,
    ) -> Result<AwsResponse, AwsServiceError> {
        // Check for If-Match conditional on DELETE
        let if_match = req
            .headers
            .get("if-match")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let mut state = self.state.write();
        let b = state
            .buckets
            .get_mut(bucket)
            .ok_or_else(|| no_such_bucket(bucket))?;

        if let Some(ref if_match_val) = if_match {
            match b.objects.get(key) {
                Some(existing) => {
                    let existing_etag = format!("\"{}\"", existing.etag);
                    if !etag_matches(if_match_val, &existing_etag) {
                        return Err(precondition_failed("If-Match"));
                    }
                }
                None => {
                    return Err(no_such_key(key));
                }
            }
        }

        // S3 returns 204 even if the key doesn't exist
        b.objects.remove(key);
        Ok(AwsResponse {
            status: StatusCode::NO_CONTENT,
            content_type: "application/xml".to_string(),
            body: Bytes::new(),
            headers: HeaderMap::new(),
        })
    }

    fn head_object(
        &self,
        req: &AwsRequest,
        bucket: &str,
        key: &str,
    ) -> Result<AwsResponse, AwsServiceError> {
        let state = self.state.read();
        let b = state
            .buckets
            .get(bucket)
            .ok_or_else(|| no_such_bucket(bucket))?;
        let obj = b.objects.get(key).ok_or_else(|| no_such_key(key))?;

        // Conditional checks for HEAD
        check_head_conditionals(req, obj)?;

        let mut headers = HeaderMap::new();
        headers.insert("etag", format!("\"{}\"", obj.etag).parse().unwrap());
        headers.insert(
            "last-modified",
            obj.last_modified
                .format("%a, %d %b %Y %H:%M:%S GMT")
                .to_string()
                .parse()
                .unwrap(),
        );

        // Handle PartNumber query param for multipart objects
        if let Some(part_num_str) = req.query_params.get("partNumber") {
            if let Ok(part_num) = part_num_str.parse::<u32>() {
                if let Some(ref part_sizes) = obj.part_sizes {
                    for &(pn, sz) in part_sizes {
                        if pn == part_num {
                            headers.insert("content-length", sz.to_string().parse().unwrap());
                            break;
                        }
                    }
                }
                if let Some(pc) = obj.parts_count {
                    headers.insert("x-amz-mp-parts-count", pc.to_string().parse().unwrap());
                }
            }
            if !headers.contains_key("content-length") {
                headers.insert("content-length", obj.size.to_string().parse().unwrap());
            }
        } else {
            headers.insert("content-length", obj.size.to_string().parse().unwrap());
        }

        for (k, v) in &obj.metadata {
            if let (Ok(name), Ok(val)) = (
                format!("x-amz-meta-{k}").parse::<http::header::HeaderName>(),
                v.parse::<http::header::HeaderValue>(),
            ) {
                headers.insert(name, val);
            }
        }

        if obj.storage_class != "STANDARD" {
            headers.insert("x-amz-storage-class", obj.storage_class.parse().unwrap());
        }

        Ok(AwsResponse {
            status: StatusCode::OK,
            content_type: obj.content_type.clone(),
            body: Bytes::new(),
            headers,
        })
    }

    fn copy_object(
        &self,
        req: &AwsRequest,
        dest_bucket: &str,
        dest_key: &str,
    ) -> Result<AwsResponse, AwsServiceError> {
        let copy_source = req
            .headers
            .get("x-amz-copy-source")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                AwsServiceError::aws_error(
                    StatusCode::BAD_REQUEST,
                    "InvalidArgument",
                    "x-amz-copy-source header is required",
                )
            })?;

        let decoded = percent_encoding::percent_decode_str(copy_source)
            .decode_utf8_lossy()
            .to_string();
        let source = decoded.strip_prefix('/').unwrap_or(&decoded);
        let source_path = source.split('?').next().unwrap_or(source);

        let (src_bucket, src_key) = source_path.split_once('/').ok_or_else(|| {
            AwsServiceError::aws_error(
                StatusCode::BAD_REQUEST,
                "InvalidArgument",
                "Invalid copy source format",
            )
        })?;

        let metadata_directive = req
            .headers
            .get("x-amz-metadata-directive")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("COPY");

        let storage_class = req
            .headers
            .get("x-amz-storage-class")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let tagging_directive = req
            .headers
            .get("x-amz-tagging-directive")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("COPY");

        let sse_algorithm = req
            .headers
            .get("x-amz-server-side-encryption")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let sse_kms_key_id = req
            .headers
            .get("x-amz-server-side-encryption-aws-kms-key-id")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let bucket_key_enabled = req
            .headers
            .get("x-amz-server-side-encryption-bucket-key-enabled")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.eq_ignore_ascii_case("true"));

        let website_redirect = req
            .headers
            .get("x-amz-website-redirect-location")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let if_none_match = req
            .headers
            .get("x-amz-copy-source-if-none-match")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let mut state = self.state.write();

        let src_obj = {
            let sb = state
                .buckets
                .get(src_bucket)
                .ok_or_else(|| no_such_bucket(src_bucket))?;
            sb.objects
                .get(src_key)
                .ok_or_else(|| {
                    AwsServiceError::aws_error_with_extra(
                        StatusCode::NOT_FOUND,
                        "NoSuchKey",
                        "The specified key does not exist.",
                        &[("Key", src_key)],
                    )
                })?
                .clone()
        };

        if let Some(ref inm) = if_none_match {
            let src_etag = format!("\"{}\"", src_obj.etag);
            if etag_matches(inm, &src_etag) {
                return Err(precondition_failed("If-None-Match"));
            }
        }

        if src_bucket == dest_bucket
            && src_key == dest_key
            && metadata_directive == "COPY"
            && storage_class.is_none()
            && sse_algorithm.is_none()
            && website_redirect.is_none()
        {
            return Err(AwsServiceError::aws_error(
                StatusCode::BAD_REQUEST,
                "InvalidRequest",
                "This copy request is illegal because it is trying to copy an object to itself \
                 without changing the object's metadata, storage class, website redirect location \
                 or encryption attributes.",
            ));
        }

        let etag = src_obj.etag.clone();
        let last_modified = Utc::now();

        let new_metadata = if metadata_directive == "REPLACE" {
            extract_user_metadata(&req.headers)
        } else {
            src_obj.metadata.clone()
        };

        let new_content_type = if metadata_directive == "REPLACE" {
            req.headers
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or(&src_obj.content_type)
                .to_string()
        } else {
            src_obj.content_type.clone()
        };

        let new_storage_class = storage_class.unwrap_or_else(|| "STANDARD".to_string());

        let new_tags = if tagging_directive == "REPLACE" {
            let th = req
                .headers
                .get("x-amz-tagging")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            parse_url_encoded_tags(th).into_iter().collect()
        } else {
            src_obj.tags.clone()
        };

        let new_sse = sse_algorithm.or_else(|| {
            if metadata_directive == "COPY" {
                src_obj.sse_algorithm.clone()
            } else {
                None
            }
        });
        let new_kms = sse_kms_key_id.or_else(|| {
            if metadata_directive == "COPY" {
                src_obj.sse_kms_key_id.clone()
            } else {
                None
            }
        });
        let new_bke = bucket_key_enabled.or(src_obj.bucket_key_enabled);
        let new_redirect = website_redirect.or_else(|| {
            if metadata_directive == "COPY" {
                src_obj.website_redirect_location.clone()
            } else {
                None
            }
        });

        let db = state
            .buckets
            .get_mut(dest_bucket)
            .ok_or_else(|| no_such_bucket(dest_bucket))?;

        let version_id = if db.versioning.as_deref() == Some("Enabled") {
            Some(uuid::Uuid::new_v4().to_string())
        } else {
            None
        };

        db.objects.insert(
            dest_key.to_string(),
            S3Object {
                key: dest_key.to_string(),
                data: src_obj.data,
                size: src_obj.size,
                etag: etag.clone(),
                last_modified,
                content_type: new_content_type,
                metadata: new_metadata,
                storage_class: new_storage_class,
                tags: new_tags,
                acl_grants: vec![],
                acl_owner_id: Some(req.account_id.clone()),
                parts_count: src_obj.parts_count,
                part_sizes: src_obj.part_sizes,
                sse_algorithm: new_sse,
                sse_kms_key_id: new_kms,
                bucket_key_enabled: new_bke,
                website_redirect_location: new_redirect,
            },
        );

        let mut response_headers = HeaderMap::new();
        if let Some(vid) = &version_id {
            response_headers.insert("x-amz-version-id", vid.parse().unwrap());
        }

        let body = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
             <CopyObjectResult>\
             <ETag>&quot;{etag}&quot;</ETag>\
             <LastModified>{}</LastModified>\
             </CopyObjectResult>",
            last_modified.format("%Y-%m-%dT%H:%M:%S%.3fZ"),
        );
        Ok(AwsResponse {
            status: StatusCode::OK,
            content_type: "text/xml".to_string(),
            body: body.into(),
            headers: response_headers,
        })
    }

    fn delete_objects(
        &self,
        req: &AwsRequest,
        bucket: &str,
    ) -> Result<AwsResponse, AwsServiceError> {
        let body_str = std::str::from_utf8(&req.body).unwrap_or("");
        let keys = parse_delete_objects_xml(body_str);

        let mut state = self.state.write();
        let b = state
            .buckets
            .get_mut(bucket)
            .ok_or_else(|| no_such_bucket(bucket))?;

        let mut deleted_xml = String::new();
        for key in &keys {
            b.objects.remove(key);
            deleted_xml.push_str(&format!(
                "<Deleted><Key>{}</Key></Deleted>",
                xml_escape(key),
            ));
        }

        let body = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
             <DeleteResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\
             {deleted_xml}\
             </DeleteResult>"
        );
        Ok(AwsResponse::xml(StatusCode::OK, body))
    }

    // ---- Object ACL ----

    fn get_object_acl(
        &self,
        req: &AwsRequest,
        bucket: &str,
        key: &str,
    ) -> Result<AwsResponse, AwsServiceError> {
        let state = self.state.read();
        let b = state
            .buckets
            .get(bucket)
            .ok_or_else(|| no_such_bucket(bucket))?;
        let obj = b.objects.get(key).ok_or_else(|| no_such_key(key))?;

        let owner_id = obj.acl_owner_id.as_deref().unwrap_or(&req.account_id);
        let body = build_acl_xml(owner_id, &obj.acl_grants, &req.account_id);
        Ok(AwsResponse::xml(StatusCode::OK, body))
    }

    fn put_object_acl(
        &self,
        req: &AwsRequest,
        bucket: &str,
        key: &str,
    ) -> Result<AwsResponse, AwsServiceError> {
        let canned = req
            .headers
            .get("x-amz-acl")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let mut state = self.state.write();
        let b = state
            .buckets
            .get_mut(bucket)
            .ok_or_else(|| no_such_bucket(bucket))?;
        let owner_id = b.acl_owner_id.clone();
        let obj = b.objects.get_mut(key).ok_or_else(|| no_such_key(key))?;

        if let Some(acl) = canned {
            obj.acl_grants = canned_acl_grants_for_object(&acl, &owner_id);
        } else {
            // Check for grant headers
            let has_grant_headers = req.headers.keys().any(|k| {
                let name = k.as_str();
                name.starts_with("x-amz-grant-")
            });
            if has_grant_headers {
                obj.acl_grants = parse_grant_headers(&req.headers);
            } else {
                // Parse from body
                let body_str = std::str::from_utf8(&req.body).unwrap_or("");
                if !body_str.is_empty() {
                    let grants = parse_acl_xml(body_str)?;
                    obj.acl_grants = grants;
                }
            }
        }

        Ok(AwsResponse {
            status: StatusCode::OK,
            content_type: "application/xml".to_string(),
            body: Bytes::new(),
            headers: HeaderMap::new(),
        })
    }

    // ---- Object Tagging ----

    fn get_object_tagging(
        &self,
        _req: &AwsRequest,
        bucket: &str,
        key: &str,
    ) -> Result<AwsResponse, AwsServiceError> {
        let state = self.state.read();
        let b = state
            .buckets
            .get(bucket)
            .ok_or_else(|| no_such_bucket(bucket))?;
        let obj = b.objects.get(key).ok_or_else(|| no_such_key(key))?;

        let mut tags_xml = String::new();
        for (k, v) in &obj.tags {
            tags_xml.push_str(&format!(
                "<Tag><Key>{}</Key><Value>{}</Value></Tag>",
                xml_escape(k),
                xml_escape(v),
            ));
        }
        let body = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
             <Tagging xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\
             <TagSet>{tags_xml}</TagSet></Tagging>"
        );
        Ok(AwsResponse::xml(StatusCode::OK, body))
    }

    fn put_object_tagging(
        &self,
        req: &AwsRequest,
        bucket: &str,
        key: &str,
    ) -> Result<AwsResponse, AwsServiceError> {
        let body_str = std::str::from_utf8(&req.body).unwrap_or("");
        let tags = parse_tagging_xml(body_str);

        // Validate: no aws: prefix
        for (k, _) in &tags {
            if k.starts_with("aws:") {
                return Err(AwsServiceError::aws_error(
                    StatusCode::BAD_REQUEST,
                    "InvalidTag",
                    "System tags cannot be added/updated by requester",
                ));
            }
        }

        // Validate: max 10 tags
        if tags.len() > 10 {
            return Err(AwsServiceError::aws_error(
                StatusCode::BAD_REQUEST,
                "BadRequest",
                "Object tags cannot be greater than 10",
            ));
        }

        let mut state = self.state.write();
        let b = state
            .buckets
            .get_mut(bucket)
            .ok_or_else(|| no_such_bucket(bucket))?;
        let obj = b
            .objects
            .get_mut(key)
            .ok_or_else(|| no_such_key_with_detail(key))?;
        obj.tags = tags.into_iter().collect();
        Ok(AwsResponse {
            status: StatusCode::OK,
            content_type: "application/xml".to_string(),
            body: Bytes::new(),
            headers: HeaderMap::new(),
        })
    }

    // ---- Multipart Upload ----

    fn create_multipart_upload(
        &self,
        req: &AwsRequest,
        bucket: &str,
        key: &str,
    ) -> Result<AwsResponse, AwsServiceError> {
        let mut state = self.state.write();
        let b = state
            .buckets
            .get_mut(bucket)
            .ok_or_else(|| no_such_bucket(bucket))?;

        let upload_id = uuid::Uuid::new_v4().to_string();
        let content_type = req
            .headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();
        let metadata = extract_user_metadata(&req.headers);
        let tagging = req
            .headers
            .get("x-amz-tagging")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let upload = MultipartUpload {
            upload_id: upload_id.clone(),
            key: key.to_string(),
            initiated: Utc::now(),
            parts: std::collections::BTreeMap::new(),
            metadata,
            content_type,
            storage_class: "STANDARD".to_string(),
            sse_algorithm: None,
            sse_kms_key_id: None,
            tagging,
            acl_grants: Vec::new(),
        };
        b.multipart_uploads.insert(upload_id.clone(), upload);

        let body = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
             <InitiateMultipartUploadResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\
             <Bucket>{}</Bucket>\
             <Key>{}</Key>\
             <UploadId>{}</UploadId>\
             </InitiateMultipartUploadResult>",
            xml_escape(bucket),
            xml_escape(key),
            xml_escape(&upload_id),
        );
        Ok(AwsResponse::xml(StatusCode::OK, body))
    }

    fn upload_part(
        &self,
        req: &AwsRequest,
        bucket: &str,
        key: &str,
        upload_id: &str,
        part_number: i64,
    ) -> Result<AwsResponse, AwsServiceError> {
        // Validate part number
        if part_number < 1 {
            return Err(no_such_upload(upload_id));
        }
        if part_number > 10000 {
            return Err(AwsServiceError::aws_error_with_extra(
                StatusCode::BAD_REQUEST,
                "InvalidArgument",
                "Part number must be an integer between 1 and 10000, inclusive",
                &[
                    ("ArgumentName", "partNumber"),
                    ("ArgumentValue", &part_number.to_string()),
                ],
            ));
        }
        let pn = part_number as u32;

        let data = req.body.clone();
        let etag = compute_md5(&data);

        let mut state = self.state.write();
        let b = state
            .buckets
            .get_mut(bucket)
            .ok_or_else(|| no_such_bucket(bucket))?;
        let upload = b
            .multipart_uploads
            .get_mut(upload_id)
            .ok_or_else(|| no_such_upload(upload_id))?;
        if upload.key != key {
            return Err(no_such_upload(upload_id));
        }

        let part = UploadPart {
            part_number: pn,
            data: data.clone(),
            etag: etag.clone(),
            size: data.len() as u64,
            last_modified: Utc::now(),
        };
        upload.parts.insert(pn, part);

        let mut headers = HeaderMap::new();
        headers.insert("etag", format!("\"{etag}\"").parse().unwrap());
        if let Some(algo) = &upload.sse_algorithm {
            headers.insert("x-amz-server-side-encryption", algo.parse().unwrap());
        }
        if let Some(kid) = &upload.sse_kms_key_id {
            headers.insert(
                "x-amz-server-side-encryption-aws-kms-key-id",
                kid.parse().unwrap(),
            );
        }
        Ok(AwsResponse {
            status: StatusCode::OK,
            content_type: "application/xml".to_string(),
            body: Bytes::new(),
            headers,
        })
    }

    fn upload_part_copy(
        &self,
        req: &AwsRequest,
        bucket: &str,
        key: &str,
        upload_id: &str,
        part_number: i64,
    ) -> Result<AwsResponse, AwsServiceError> {
        let copy_source = req
            .headers
            .get("x-amz-copy-source")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                AwsServiceError::aws_error(
                    StatusCode::BAD_REQUEST,
                    "InvalidArgument",
                    "x-amz-copy-source header is required",
                )
            })?;

        let decoded = percent_encoding::percent_decode_str(copy_source)
            .decode_utf8_lossy()
            .to_string();
        let source = decoded.strip_prefix('/').unwrap_or(&decoded);
        let (src_bucket, src_key) = source.split_once('/').ok_or_else(|| {
            AwsServiceError::aws_error(
                StatusCode::BAD_REQUEST,
                "InvalidArgument",
                "Invalid copy source format",
            )
        })?;

        let copy_range = req
            .headers
            .get("x-amz-copy-source-range")
            .and_then(|v| v.to_str().ok());

        let mut state = self.state.write();
        let src_data = {
            let sb = state
                .buckets
                .get(src_bucket)
                .ok_or_else(|| no_such_bucket(src_bucket))?;
            let src_obj = sb
                .objects
                .get(src_key)
                .ok_or_else(|| no_such_key(src_key))?;

            if let Some(range_str) = copy_range {
                let range_part = range_str.strip_prefix("bytes=").unwrap_or(range_str);
                if let Some((start_str, end_str)) = range_part.split_once('-') {
                    let start: usize = start_str.parse().unwrap_or(0);
                    let end: usize = end_str.parse().unwrap_or(src_obj.data.len() - 1);
                    let end = std::cmp::min(end + 1, src_obj.data.len());
                    src_obj.data.slice(start..end)
                } else {
                    src_obj.data.clone()
                }
            } else {
                src_obj.data.clone()
            }
        };

        let etag = compute_md5(&src_data);
        let b = state
            .buckets
            .get_mut(bucket)
            .ok_or_else(|| no_such_bucket(bucket))?;
        let upload = b
            .multipart_uploads
            .get_mut(upload_id)
            .ok_or_else(|| no_such_upload(upload_id))?;
        if upload.key != key {
            return Err(no_such_upload(upload_id));
        }

        let part = UploadPart {
            part_number: part_number as u32,
            data: src_data,
            etag: etag.clone(),
            size: 0, // will be set from data
            last_modified: Utc::now(),
        };
        upload.parts.insert(part_number as u32, part);

        let body = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
             <CopyPartResult>\
             <ETag>&quot;{etag}&quot;</ETag>\
             <LastModified>{}</LastModified>\
             </CopyPartResult>",
            Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ"),
        );
        Ok(AwsResponse::xml(StatusCode::OK, body))
    }

    fn complete_multipart_upload(
        &self,
        req: &AwsRequest,
        bucket: &str,
        key: &str,
        upload_id: &str,
    ) -> Result<AwsResponse, AwsServiceError> {
        let body_str = std::str::from_utf8(&req.body).unwrap_or("");
        let submitted_parts = parse_complete_multipart_xml(body_str);

        if submitted_parts.is_empty() {
            return Err(AwsServiceError::aws_error(
                StatusCode::BAD_REQUEST,
                "MalformedXML",
                "The XML you provided was not well-formed or did not validate against our published schema",
            ));
        }

        // Check for If-None-Match conditional
        let if_none_match = req
            .headers
            .get("if-none-match")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let mut state = self.state.write();
        let b = state
            .buckets
            .get_mut(bucket)
            .ok_or_else(|| no_such_bucket(bucket))?;

        // If-None-Match: if "*", fail if object already exists
        if let Some(ref inm) = if_none_match {
            if inm.trim() == "*" && b.objects.contains_key(key) {
                b.multipart_uploads.remove(upload_id);
                return Err(precondition_failed("If-None-Match"));
            }
        }

        let upload = match b.multipart_uploads.get(upload_id) {
            Some(u) => u.clone(),
            None => {
                // Upload already completed - return existing object if it exists
                if let Some(obj) = b.objects.get(key) {
                    let etag = obj.etag.clone();
                    let body = format!(
                        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
                         <CompleteMultipartUploadResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\
                         <Bucket>{}</Bucket>\
                         <Key>{}</Key>\
                         <ETag>&quot;{}&quot;</ETag>\
                         </CompleteMultipartUploadResult>",
                        xml_escape(bucket),
                        xml_escape(key),
                        xml_escape(&etag),
                    );
                    return Ok(AwsResponse {
                        status: StatusCode::OK,
                        content_type: "text/xml".to_string(),
                        body: body.into(),
                        headers: HeaderMap::new(),
                    });
                }
                return Err(no_such_upload(upload_id));
            }
        };

        if upload.key != key {
            return Err(no_such_upload(upload_id));
        }

        // Sort submitted parts by part number
        let mut sorted_parts = submitted_parts;
        sorted_parts.sort_by_key(|p| p.0);

        // Assemble the object from parts
        let mut combined_data = Vec::new();
        let mut md5_digests = Vec::new();
        let mut part_sizes = Vec::new();

        for (part_num, _submitted_etag) in &sorted_parts {
            let part = upload.parts.get(part_num).ok_or_else(|| {
                AwsServiceError::aws_error(
                    StatusCode::BAD_REQUEST,
                    "InvalidPart",
                    "One or more of the specified parts could not be found.",
                )
            })?;
            combined_data.extend_from_slice(&part.data);
            let part_md5 = Md5::digest(&part.data);
            md5_digests.extend_from_slice(&part_md5);
            part_sizes.push((*part_num, part.size));
        }

        // Multipart ETag: MD5(concat(part_md5_digests))-N
        let combined_md5 = Md5::digest(&md5_digests);
        let etag = format!("{:x}-{}", combined_md5, sorted_parts.len());
        let data = Bytes::from(combined_data);

        let tags = if let Some(ref tagging) = upload.tagging {
            parse_url_encoded_tags(tagging).into_iter().collect()
        } else {
            std::collections::HashMap::new()
        };

        let version_id = if b.versioning.as_deref() == Some("Enabled") {
            Some(uuid::Uuid::new_v4().to_string())
        } else {
            None
        };

        let obj = S3Object {
            key: key.to_string(),
            size: data.len() as u64,
            data,
            content_type: upload.content_type.clone(),
            etag: etag.clone(),
            last_modified: Utc::now(),
            metadata: upload.metadata.clone(),
            storage_class: upload.storage_class.clone(),
            tags,
            acl_grants: upload.acl_grants.clone(),
            acl_owner_id: Some(b.acl_owner_id.clone()),
            parts_count: Some(sorted_parts.len() as u32),
            part_sizes: Some(part_sizes),
            sse_algorithm: upload.sse_algorithm.clone(),
            sse_kms_key_id: upload.sse_kms_key_id.clone(),
            bucket_key_enabled: None,
            website_redirect_location: None,
        };
        b.objects.insert(key.to_string(), obj);
        b.multipart_uploads.remove(upload_id);

        let mut headers = HeaderMap::new();
        if let Some(vid) = &version_id {
            headers.insert("x-amz-version-id", vid.parse().unwrap());
        }
        if let Some(algo) = &upload.sse_algorithm {
            headers.insert("x-amz-server-side-encryption", algo.parse().unwrap());
        }
        if let Some(kid) = &upload.sse_kms_key_id {
            headers.insert(
                "x-amz-server-side-encryption-aws-kms-key-id",
                kid.parse().unwrap(),
            );
        }

        let body = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
             <CompleteMultipartUploadResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\
             <Bucket>{}</Bucket>\
             <Key>{}</Key>\
             <ETag>&quot;{}&quot;</ETag>\
             </CompleteMultipartUploadResult>",
            xml_escape(bucket),
            xml_escape(key),
            xml_escape(&etag),
        );
        Ok(AwsResponse {
            status: StatusCode::OK,
            content_type: "text/xml".to_string(),
            body: body.into(),
            headers,
        })
    }

    fn abort_multipart_upload(
        &self,
        bucket: &str,
        _key: &str,
        upload_id: &str,
    ) -> Result<AwsResponse, AwsServiceError> {
        let mut state = self.state.write();
        let b = state
            .buckets
            .get_mut(bucket)
            .ok_or_else(|| no_such_bucket(bucket))?;

        if b.multipart_uploads.remove(upload_id).is_none() {
            return Err(no_such_upload(upload_id));
        }

        Ok(AwsResponse {
            status: StatusCode::NO_CONTENT,
            content_type: "application/xml".to_string(),
            body: Bytes::new(),
            headers: HeaderMap::new(),
        })
    }

    fn list_multipart_uploads(&self, bucket: &str) -> Result<AwsResponse, AwsServiceError> {
        let state = self.state.read();
        let b = state
            .buckets
            .get(bucket)
            .ok_or_else(|| no_such_bucket(bucket))?;

        let mut uploads_xml = String::new();
        for upload in b.multipart_uploads.values() {
            uploads_xml.push_str(&format!(
                "<Upload>\
                 <Key>{}</Key>\
                 <UploadId>{}</UploadId>\
                 <Initiated>{}</Initiated>\
                 <StorageClass>{}</StorageClass>\
                 </Upload>",
                xml_escape(&upload.key),
                xml_escape(&upload.upload_id),
                upload.initiated.format("%Y-%m-%dT%H:%M:%S%.3fZ"),
                xml_escape(&upload.storage_class),
            ));
        }

        let body = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
             <ListMultipartUploadsResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\
             <Bucket>{}</Bucket>\
             <MaxUploads>1000</MaxUploads>\
             <IsTruncated>false</IsTruncated>\
             {uploads_xml}\
             </ListMultipartUploadsResult>",
            xml_escape(bucket),
        );
        Ok(AwsResponse::xml(StatusCode::OK, body))
    }

    fn list_parts(
        &self,
        _req: &AwsRequest,
        bucket: &str,
        key: &str,
        upload_id: &str,
    ) -> Result<AwsResponse, AwsServiceError> {
        let state = self.state.read();
        let b = state
            .buckets
            .get(bucket)
            .ok_or_else(|| no_such_bucket(bucket))?;
        let upload = b
            .multipart_uploads
            .get(upload_id)
            .ok_or_else(|| no_such_upload(upload_id))?;
        if upload.key != key {
            return Err(no_such_upload(upload_id));
        }

        let mut parts_xml = String::new();
        for part in upload.parts.values() {
            parts_xml.push_str(&format!(
                "<Part>\
                 <PartNumber>{}</PartNumber>\
                 <ETag>&quot;{}&quot;</ETag>\
                 <Size>{}</Size>\
                 <LastModified>{}</LastModified>\
                 </Part>",
                part.part_number,
                xml_escape(&part.etag),
                part.size,
                part.last_modified.format("%Y-%m-%dT%H:%M:%S%.3fZ"),
            ));
        }

        let body = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
             <ListPartsResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\
             <Bucket>{}</Bucket>\
             <Key>{}</Key>\
             <UploadId>{}</UploadId>\
             <IsTruncated>false</IsTruncated>\
             {parts_xml}\
             </ListPartsResult>",
            xml_escape(bucket),
            xml_escape(key),
            xml_escape(upload_id),
        );
        Ok(AwsResponse::xml(StatusCode::OK, body))
    }

    fn delete_object_tagging(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<AwsResponse, AwsServiceError> {
        let mut state = self.state.write();
        let b = state
            .buckets
            .get_mut(bucket)
            .ok_or_else(|| no_such_bucket(bucket))?;
        let obj = b.objects.get_mut(key).ok_or_else(|| no_such_key(key))?;
        obj.tags.clear();
        Ok(AwsResponse {
            status: StatusCode::NO_CONTENT,
            content_type: "application/xml".to_string(),
            body: Bytes::new(),
            headers: HeaderMap::new(),
        })
    }
}

// ---------------------------------------------------------------------------
// Conditional request helpers
// ---------------------------------------------------------------------------

/// Truncate a DateTime to second-level precision (HTTP dates have no sub-second info).
fn truncate_to_seconds(dt: DateTime<Utc>) -> DateTime<Utc> {
    dt.with_nanosecond(0).unwrap_or(dt)
}

fn check_get_conditionals(req: &AwsRequest, obj: &S3Object) -> Result<(), AwsServiceError> {
    let obj_etag = format!("\"{}\"", obj.etag);
    let obj_time = truncate_to_seconds(obj.last_modified);

    // If-Match
    if let Some(if_match) = req.headers.get("if-match").and_then(|v| v.to_str().ok()) {
        if !etag_matches(if_match, &obj_etag) {
            return Err(precondition_failed("If-Match"));
        }
    }

    // If-None-Match
    if let Some(if_none_match) = req
        .headers
        .get("if-none-match")
        .and_then(|v| v.to_str().ok())
    {
        if etag_matches(if_none_match, &obj_etag) {
            return Err(not_modified_with_etag(&obj_etag));
        }
    }

    // If-Unmodified-Since
    if let Some(since) = req
        .headers
        .get("if-unmodified-since")
        .and_then(|v| v.to_str().ok())
    {
        if let Some(dt) = parse_http_date(since) {
            if obj_time > dt {
                return Err(precondition_failed("If-Unmodified-Since"));
            }
        }
    }

    // If-Modified-Since
    if let Some(since) = req
        .headers
        .get("if-modified-since")
        .and_then(|v| v.to_str().ok())
    {
        if let Some(dt) = parse_http_date(since) {
            if obj_time <= dt {
                return Err(not_modified());
            }
        }
    }

    Ok(())
}

fn check_head_conditionals(req: &AwsRequest, obj: &S3Object) -> Result<(), AwsServiceError> {
    let obj_etag = format!("\"{}\"", obj.etag);
    let obj_time = truncate_to_seconds(obj.last_modified);

    // If-Match
    if let Some(if_match) = req.headers.get("if-match").and_then(|v| v.to_str().ok()) {
        if !etag_matches(if_match, &obj_etag) {
            return Err(AwsServiceError::aws_error(
                StatusCode::PRECONDITION_FAILED,
                "412",
                "Precondition Failed",
            ));
        }
    }

    // If-None-Match
    if let Some(if_none_match) = req
        .headers
        .get("if-none-match")
        .and_then(|v| v.to_str().ok())
    {
        if etag_matches(if_none_match, &obj_etag) {
            return Err(not_modified_with_etag(&obj_etag));
        }
    }

    // If-Unmodified-Since
    if let Some(since) = req
        .headers
        .get("if-unmodified-since")
        .and_then(|v| v.to_str().ok())
    {
        if let Some(dt) = parse_http_date(since) {
            if obj_time > dt {
                return Err(AwsServiceError::aws_error(
                    StatusCode::PRECONDITION_FAILED,
                    "412",
                    "Precondition Failed",
                ));
            }
        }
    }

    // If-Modified-Since
    if let Some(since) = req
        .headers
        .get("if-modified-since")
        .and_then(|v| v.to_str().ok())
    {
        if let Some(dt) = parse_http_date(since) {
            if obj_time <= dt {
                return Err(not_modified());
            }
        }
    }

    Ok(())
}

fn etag_matches(condition: &str, obj_etag: &str) -> bool {
    let condition = condition.trim();
    if condition == "*" {
        return true;
    }
    // Strip quotes from both for comparison
    let clean_condition = condition.replace('"', "");
    let clean_etag = obj_etag.replace('"', "");
    clean_condition == clean_etag
}

fn parse_http_date(s: &str) -> Option<DateTime<Utc>> {
    // Try RFC 2822 format: "Sat, 01 Jan 2000 00:00:00 GMT"
    if let Ok(dt) = DateTime::parse_from_rfc2822(s) {
        return Some(dt.with_timezone(&Utc));
    }
    // Try RFC 3339
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    // Try common HTTP date format: "%a, %d %b %Y %H:%M:%S GMT"
    if let Ok(dt) =
        chrono::NaiveDateTime::parse_from_str(s.trim_end_matches(" GMT"), "%a, %d %b %Y %H:%M:%S")
    {
        return Some(dt.and_utc());
    }
    // Try ISO 8601
    if let Ok(dt) = s.parse::<DateTime<Utc>>() {
        return Some(dt);
    }
    None
}

fn not_modified() -> AwsServiceError {
    AwsServiceError::aws_error(StatusCode::NOT_MODIFIED, "304", "Not Modified")
}

fn not_modified_with_etag(etag: &str) -> AwsServiceError {
    AwsServiceError::aws_error_with_headers(
        StatusCode::NOT_MODIFIED,
        "304",
        "Not Modified",
        vec![("etag".to_string(), etag.to_string())],
    )
}

fn precondition_failed(condition: &str) -> AwsServiceError {
    AwsServiceError::aws_error_with_extra(
        StatusCode::PRECONDITION_FAILED,
        "PreconditionFailed",
        "At least one of the pre-conditions you specified did not hold",
        &[("Condition", condition)],
    )
}

// ---------------------------------------------------------------------------
// ACL helpers
// ---------------------------------------------------------------------------

fn build_acl_xml(owner_id: &str, grants: &[AclGrant], _account_id: &str) -> String {
    let mut grants_xml = String::new();
    for g in grants {
        let grantee_xml = if g.grantee_type == "Group" {
            let uri = g.grantee_uri.as_deref().unwrap_or("");
            format!(
                "<Grantee xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xsi:type=\"Group\">\
                 <URI>{}</URI></Grantee>",
                xml_escape(uri),
            )
        } else {
            let id = g.grantee_id.as_deref().unwrap_or("");
            format!(
                "<Grantee xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xsi:type=\"CanonicalUser\">\
                 <ID>{}</ID></Grantee>",
                xml_escape(id),
            )
        };
        grants_xml.push_str(&format!(
            "<Grant>{grantee_xml}<Permission>{}</Permission></Grant>",
            xml_escape(&g.permission),
        ));
    }

    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
         <AccessControlPolicy xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\
         <Owner><ID>{owner_id}</ID><DisplayName>{owner_id}</DisplayName></Owner>\
         <AccessControlList>{grants_xml}</AccessControlList>\
         </AccessControlPolicy>",
        owner_id = xml_escape(owner_id),
    )
}

fn canned_acl_grants(acl: &str, owner_id: &str) -> Vec<AclGrant> {
    let owner_grant = AclGrant {
        grantee_type: "CanonicalUser".to_string(),
        grantee_id: Some(owner_id.to_string()),
        grantee_display_name: Some(owner_id.to_string()),
        grantee_uri: None,
        permission: "FULL_CONTROL".to_string(),
    };
    match acl {
        "private" => vec![owner_grant],
        "public-read" => vec![
            owner_grant,
            AclGrant {
                grantee_type: "Group".to_string(),
                grantee_id: None,
                grantee_display_name: None,
                grantee_uri: Some("http://acs.amazonaws.com/groups/global/AllUsers".to_string()),
                permission: "READ".to_string(),
            },
        ],
        "public-read-write" => vec![
            owner_grant,
            AclGrant {
                grantee_type: "Group".to_string(),
                grantee_id: None,
                grantee_display_name: None,
                grantee_uri: Some("http://acs.amazonaws.com/groups/global/AllUsers".to_string()),
                permission: "READ".to_string(),
            },
            AclGrant {
                grantee_type: "Group".to_string(),
                grantee_id: None,
                grantee_display_name: None,
                grantee_uri: Some("http://acs.amazonaws.com/groups/global/AllUsers".to_string()),
                permission: "WRITE".to_string(),
            },
        ],
        "authenticated-read" => vec![
            owner_grant,
            AclGrant {
                grantee_type: "Group".to_string(),
                grantee_id: None,
                grantee_display_name: None,
                grantee_uri: Some(
                    "http://acs.amazonaws.com/groups/global/AuthenticatedUsers".to_string(),
                ),
                permission: "READ".to_string(),
            },
        ],
        "bucket-owner-full-control" => vec![owner_grant],
        _ => vec![owner_grant],
    }
}

fn canned_acl_grants_for_object(acl: &str, owner_id: &str) -> Vec<AclGrant> {
    // For objects, canned ACLs work the same way
    canned_acl_grants(acl, owner_id)
}

fn parse_grant_headers(headers: &HeaderMap) -> Vec<AclGrant> {
    let mut grants = Vec::new();
    let header_permission_map = [
        ("x-amz-grant-read", "READ"),
        ("x-amz-grant-write", "WRITE"),
        ("x-amz-grant-read-acp", "READ_ACP"),
        ("x-amz-grant-write-acp", "WRITE_ACP"),
        ("x-amz-grant-full-control", "FULL_CONTROL"),
    ];

    for (header, permission) in &header_permission_map {
        if let Some(value) = headers.get(*header).and_then(|v| v.to_str().ok()) {
            // Parse "id=xxx" or "uri=xxx" or "emailAddress=xxx"
            for part in value.split(',') {
                let part = part.trim();
                if let Some((key, val)) = part.split_once('=') {
                    let val = val.trim().trim_matches('"');
                    let key = key.trim().to_lowercase();
                    match key.as_str() {
                        "id" => {
                            grants.push(AclGrant {
                                grantee_type: "CanonicalUser".to_string(),
                                grantee_id: Some(val.to_string()),
                                grantee_display_name: Some(val.to_string()),
                                grantee_uri: None,
                                permission: permission.to_string(),
                            });
                        }
                        "uri" | "url" => {
                            grants.push(AclGrant {
                                grantee_type: "Group".to_string(),
                                grantee_id: None,
                                grantee_display_name: None,
                                grantee_uri: Some(val.to_string()),
                                permission: permission.to_string(),
                            });
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    grants
}

fn parse_acl_xml(xml: &str) -> Result<Vec<AclGrant>, AwsServiceError> {
    // Check for Owner presence
    if xml.contains("<AccessControlPolicy") && !xml.contains("<Owner>") {
        return Err(AwsServiceError::aws_error(
            StatusCode::BAD_REQUEST,
            "MalformedACLError",
            "The XML you provided was not well-formed or did not validate against our published schema",
        ));
    }

    let valid_permissions = ["READ", "WRITE", "READ_ACP", "WRITE_ACP", "FULL_CONTROL"];

    let mut grants = Vec::new();
    let mut remaining = xml;
    while let Some(start) = remaining.find("<Grant>") {
        let after = &remaining[start + 7..];
        if let Some(end) = after.find("</Grant>") {
            let grant_body = &after[..end];

            // Extract permission
            let permission = extract_xml_value(grant_body, "Permission").unwrap_or_default();
            if !valid_permissions.contains(&permission.as_str()) {
                return Err(AwsServiceError::aws_error(
                    StatusCode::BAD_REQUEST,
                    "MalformedACLError",
                    "The XML you provided was not well-formed or did not validate against our published schema",
                ));
            }

            // Determine grantee type
            if grant_body.contains("xsi:type=\"Group\"") || grant_body.contains("<URI>") {
                let uri = extract_xml_value(grant_body, "URI").unwrap_or_default();
                grants.push(AclGrant {
                    grantee_type: "Group".to_string(),
                    grantee_id: None,
                    grantee_display_name: None,
                    grantee_uri: Some(uri),
                    permission,
                });
            } else {
                let id = extract_xml_value(grant_body, "ID").unwrap_or_default();
                let display =
                    extract_xml_value(grant_body, "DisplayName").unwrap_or_else(|| id.clone());
                grants.push(AclGrant {
                    grantee_type: "CanonicalUser".to_string(),
                    grantee_id: Some(id),
                    grantee_display_name: Some(display),
                    grantee_uri: None,
                    permission,
                });
            }

            remaining = &after[end + 8..];
        } else {
            break;
        }
    }
    Ok(grants)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn no_such_bucket(_bucket: &str) -> AwsServiceError {
    AwsServiceError::aws_error(
        StatusCode::NOT_FOUND,
        "NoSuchBucket",
        "The specified bucket does not exist",
    )
}

fn no_such_key(key: &str) -> AwsServiceError {
    AwsServiceError::aws_error(
        StatusCode::NOT_FOUND,
        "NoSuchKey",
        format!("The specified key does not exist: {key}"),
    )
}

fn no_such_upload(upload_id: &str) -> AwsServiceError {
    AwsServiceError::aws_error_with_extra(
        StatusCode::NOT_FOUND,
        "NoSuchUpload",
        "The specified upload does not exist. The upload ID may be invalid, \
         or the upload may have been aborted or completed.",
        &[("UploadId", upload_id)],
    )
}

fn no_such_key_with_detail(key: &str) -> AwsServiceError {
    AwsServiceError::aws_error_with_extra(
        StatusCode::NOT_FOUND,
        "NoSuchKey",
        "The specified key does not exist.",
        &[("Key", key)],
    )
}

fn compute_md5(data: &[u8]) -> String {
    let digest = Md5::digest(data);
    format!("{:x}", digest)
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn extract_user_metadata(headers: &HeaderMap) -> std::collections::HashMap<String, String> {
    let mut meta = std::collections::HashMap::new();
    for (name, value) in headers {
        if let Some(key) = name.as_str().strip_prefix("x-amz-meta-") {
            if let Ok(v) = value.to_str() {
                meta.insert(key.to_string(), v.to_string());
            }
        }
    }
    meta
}

fn is_valid_bucket_name(name: &str) -> bool {
    if name.len() < 3 || name.len() > 63 {
        return false;
    }
    // Must start and end with alphanumeric
    let bytes = name.as_bytes();
    if !bytes[0].is_ascii_alphanumeric() || !bytes[bytes.len() - 1].is_ascii_alphanumeric() {
        return false;
    }
    // Only lowercase letters, digits, hyphens, dots
    name.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.')
}

fn is_valid_region(region: &str) -> bool {
    // Basic validation: region should match pattern like us-east-1, eu-west-2, etc.
    let valid_regions = [
        "us-east-1",
        "us-east-2",
        "us-west-1",
        "us-west-2",
        "af-south-1",
        "ap-east-1",
        "ap-south-1",
        "ap-south-2",
        "ap-southeast-1",
        "ap-southeast-2",
        "ap-southeast-3",
        "ap-southeast-4",
        "ap-northeast-1",
        "ap-northeast-2",
        "ap-northeast-3",
        "ca-central-1",
        "ca-west-1",
        "eu-central-1",
        "eu-central-2",
        "eu-west-1",
        "eu-west-2",
        "eu-west-3",
        "eu-south-1",
        "eu-south-2",
        "eu-north-1",
        "il-central-1",
        "me-south-1",
        "me-central-1",
        "sa-east-1",
    ];
    valid_regions.contains(&region)
}

/// Minimal XML parser for `<Delete><Object><Key>...</Key></Object>...</Delete>`.
fn parse_delete_objects_xml(xml: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut remaining = xml;
    while let Some(start) = remaining.find("<Key>") {
        let after = &remaining[start + 5..];
        if let Some(end) = after.find("</Key>") {
            keys.push(after[..end].to_string());
            remaining = &after[end + 6..];
        } else {
            break;
        }
    }
    keys
}

/// Minimal XML parser for `<Tagging><TagSet><Tag><Key>k</Key><Value>v</Value></Tag>...`.
/// Returns a Vec to preserve insertion order and detect duplicates.
fn parse_tagging_xml(xml: &str) -> Vec<(String, String)> {
    let mut tags = Vec::new();
    let mut remaining = xml;
    while let Some(tag_start) = remaining.find("<Tag>") {
        let after = &remaining[tag_start + 5..];
        if let Some(tag_end) = after.find("</Tag>") {
            let tag_body = &after[..tag_end];
            let key = extract_xml_value(tag_body, "Key");
            let value = extract_xml_value(tag_body, "Value");
            if let (Some(k), Some(v)) = (key, value) {
                tags.push((k, v));
            }
            remaining = &after[tag_end + 6..];
        } else {
            break;
        }
    }
    tags
}

fn validate_tags(tags: &[(String, String)]) -> Result<(), AwsServiceError> {
    // Check for duplicate keys
    let mut seen = std::collections::HashSet::new();
    for (k, _) in tags {
        if !seen.insert(k.as_str()) {
            return Err(AwsServiceError::aws_error(
                StatusCode::BAD_REQUEST,
                "InvalidTag",
                "Cannot provide multiple Tags with the same key",
            ));
        }
        // Check for aws: prefix
        if k.starts_with("aws:") {
            return Err(AwsServiceError::aws_error(
                StatusCode::BAD_REQUEST,
                "InvalidTag",
                "System tags cannot be added/updated by requester",
            ));
        }
    }
    Ok(())
}

fn extract_xml_value(xml: &str, tag: &str) -> Option<String> {
    // Handle self-closing tags like <Value /> or <Value/>
    let self_closing1 = format!("<{tag} />");
    let self_closing2 = format!("<{tag}/>");
    if xml.contains(&self_closing1) || xml.contains(&self_closing2) {
        // Check if the self-closing tag appears before any open+close pair
        let self_pos = xml
            .find(&self_closing1)
            .or_else(|| xml.find(&self_closing2));
        let open = format!("<{tag}>");
        let open_pos = xml.find(&open);
        match (self_pos, open_pos) {
            (Some(sp), Some(op)) if sp < op => return Some(String::new()),
            (Some(_), None) => return Some(String::new()),
            _ => {}
        }
    }

    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml.find(&close)?;
    Some(xml[start..end].to_string())
}

/// Parse the CompleteMultipartUpload XML body into (part_number, etag) pairs.
fn parse_complete_multipart_xml(xml: &str) -> Vec<(u32, String)> {
    let mut parts = Vec::new();
    let mut remaining = xml;
    while let Some(part_start) = remaining.find("<Part>") {
        let after = &remaining[part_start + 6..];
        if let Some(part_end) = after.find("</Part>") {
            let part_body = &after[..part_end];
            let part_num =
                extract_xml_value(part_body, "PartNumber").and_then(|s| s.parse::<u32>().ok());
            let etag = extract_xml_value(part_body, "ETag").map(|s| s.replace('"', ""));
            if let (Some(num), Some(e)) = (part_num, etag) {
                parts.push((num, e));
            }
            remaining = &after[part_end + 7..];
        } else {
            break;
        }
    }
    parts
}

fn parse_url_encoded_tags(s: &str) -> Vec<(String, String)> {
    let mut tags = Vec::new();
    for pair in s.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = match pair.find('=') {
            Some(pos) => (&pair[..pos], &pair[pos + 1..]),
            None => (pair, ""),
        };
        tags.push((
            percent_encoding::percent_decode_str(key)
                .decode_utf8_lossy()
                .to_string(),
            percent_encoding::percent_decode_str(value)
                .decode_utf8_lossy()
                .to_string(),
        ));
    }
    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_bucket_names() {
        assert!(is_valid_bucket_name("my-bucket"));
        assert!(is_valid_bucket_name("my.bucket.name"));
        assert!(is_valid_bucket_name("abc"));
        assert!(!is_valid_bucket_name("ab"));
        assert!(!is_valid_bucket_name("-bucket"));
        assert!(!is_valid_bucket_name("Bucket"));
        assert!(!is_valid_bucket_name("bucket-"));
    }

    #[test]
    fn parse_delete_xml() {
        let xml = r#"<Delete><Object><Key>a.txt</Key></Object><Object><Key>b/c.txt</Key></Object></Delete>"#;
        let keys = parse_delete_objects_xml(xml);
        assert_eq!(keys, vec!["a.txt", "b/c.txt"]);
    }

    #[test]
    fn parse_tags_xml() {
        let xml =
            r#"<Tagging><TagSet><Tag><Key>env</Key><Value>prod</Value></Tag></TagSet></Tagging>"#;
        let tags = parse_tagging_xml(xml);
        assert_eq!(tags, vec![("env".to_string(), "prod".to_string())]);
    }

    #[test]
    fn md5_hash() {
        let hash = compute_md5(b"hello");
        assert_eq!(hash, "5d41402abc4b2a76b9719d911017c592");
    }

    #[test]
    fn test_etag_matches() {
        assert!(etag_matches("\"abc\"", "\"abc\""));
        assert!(etag_matches("abc", "\"abc\""));
        assert!(etag_matches("*", "\"abc\""));
        assert!(!etag_matches("\"xyz\"", "\"abc\""));
    }
}
