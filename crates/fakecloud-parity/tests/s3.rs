mod common;

use aws_sdk_s3::primitives::ByteStream;
use common::{retry, unique_name, Backend};

#[tokio::test]
async fn s3_put_get_head_list_delete() {
    let backend = Backend::from_env().await;
    let s3 = backend.s3().await;
    // S3 bucket names have stricter rules than other resource names:
    // lowercase only, no underscores, 3-63 chars, globally unique.
    let bucket = unique_name("s3").to_lowercase().replace('_', "-");
    assert!(bucket.len() <= 63, "s3 bucket name too long: {bucket}");

    // Create. In us-east-1, CreateBucket must NOT set a LocationConstraint.
    // fakecloud accepts either, but real us-east-1 rejects a LocationConstraint.
    s3.create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket");

    let key = "parity/object.txt";
    let body = b"hello s3 parity".to_vec();

    s3.put_object()
        .bucket(&bucket)
        .key(key)
        .body(ByteStream::from(body.clone()))
        .send()
        .await
        .expect("put_object");

    // HeadObject -> content length matches.
    let head = retry(10, 200, || async {
        s3.head_object().bucket(&bucket).key(key).send().await
    })
    .await
    .expect("head_object");
    assert_eq!(head.content_length().unwrap_or(0), body.len() as i64);

    // GetObject -> byte-for-byte round-trip.
    let get = s3
        .get_object()
        .bucket(&bucket)
        .key(key)
        .send()
        .await
        .expect("get_object");
    let got = get
        .body
        .collect()
        .await
        .expect("collect get_object body")
        .into_bytes();
    assert_eq!(got.as_ref(), body.as_slice());

    // List -> contains the key we wrote.
    let list = s3
        .list_objects_v2()
        .bucket(&bucket)
        .send()
        .await
        .expect("list_objects_v2");
    let keys: Vec<&str> = list.contents().iter().filter_map(|o| o.key()).collect();
    assert!(
        keys.contains(&key),
        "expected {key} in listing, got {keys:?}"
    );

    // Teardown.
    s3.delete_object()
        .bucket(&bucket)
        .key(key)
        .send()
        .await
        .expect("delete_object");
    s3.delete_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("delete_bucket");
}

#[tokio::test]
async fn s3_get_object_nonexistent_returns_expected_error() {
    let backend = Backend::from_env().await;
    let s3 = backend.s3().await;
    let bucket = unique_name("s3-missing").to_lowercase().replace('_', "-");

    s3.create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket");

    let err = s3
        .get_object()
        .bucket(&bucket)
        .key("does-not-exist")
        .send()
        .await
        .expect_err("get_object on missing key should fail");
    let code = err
        .into_service_error()
        .meta()
        .code()
        .unwrap_or_default()
        .to_string();
    assert!(code == "NoSuchKey", "expected NoSuchKey, got code={code:?}");

    s3.delete_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("delete_bucket");
}

#[tokio::test]
async fn s3_multipart_upload_roundtrip() {
    let backend = Backend::from_env().await;
    let s3 = backend.s3().await;
    let bucket = unique_name("s3-mpu").to_lowercase().replace('_', "-");
    let key = "multipart/object.bin";

    s3.create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket");

    // 5 MiB is the minimum non-final part size on real S3. Use two parts
    // of 5 MiB + one 1 KiB tail, so we exercise the "last part can be
    // smaller" semantics.
    let part1: Vec<u8> = vec![0xAA; 5 * 1024 * 1024];
    let part2: Vec<u8> = vec![0xBB; 5 * 1024 * 1024];
    let tail: Vec<u8> = vec![0xCC; 1024];

    let init = s3
        .create_multipart_upload()
        .bucket(&bucket)
        .key(key)
        .send()
        .await
        .expect("create_multipart_upload");
    let upload_id = init.upload_id().expect("upload_id").to_string();

    let mut completed_parts = Vec::new();
    for (i, chunk) in [part1.clone(), part2.clone(), tail.clone()]
        .iter()
        .enumerate()
    {
        let part_number = (i as i32) + 1;
        let resp = s3
            .upload_part()
            .bucket(&bucket)
            .key(key)
            .upload_id(&upload_id)
            .part_number(part_number)
            .body(ByteStream::from(chunk.clone()))
            .send()
            .await
            .expect("upload_part");
        completed_parts.push(
            aws_sdk_s3::types::CompletedPart::builder()
                .part_number(part_number)
                .e_tag(resp.e_tag().unwrap_or_default())
                .build(),
        );
    }

    s3.complete_multipart_upload()
        .bucket(&bucket)
        .key(key)
        .upload_id(&upload_id)
        .multipart_upload(
            aws_sdk_s3::types::CompletedMultipartUpload::builder()
                .set_parts(Some(completed_parts))
                .build(),
        )
        .send()
        .await
        .expect("complete_multipart_upload");

    // Round-trip: read the object back and assert full byte equality.
    let got = s3
        .get_object()
        .bucket(&bucket)
        .key(key)
        .send()
        .await
        .expect("get_object");
    let body = got.body.collect().await.expect("collect body").into_bytes();

    let mut expected = Vec::with_capacity(part1.len() + part2.len() + tail.len());
    expected.extend_from_slice(&part1);
    expected.extend_from_slice(&part2);
    expected.extend_from_slice(&tail);
    assert_eq!(
        body.len(),
        expected.len(),
        "multipart object length mismatch"
    );
    assert_eq!(body.as_ref(), expected.as_slice());

    s3.delete_object()
        .bucket(&bucket)
        .key(key)
        .send()
        .await
        .expect("delete_object");
    s3.delete_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("delete_bucket");
}
