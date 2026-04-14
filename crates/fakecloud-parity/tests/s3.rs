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
