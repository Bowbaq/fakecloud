mod common;

use aws_sdk_kms::primitives::Blob;
use common::Backend;

#[tokio::test]
async fn kms_encrypt_decrypt_roundtrip() {
    let backend = Backend::from_env().await;
    let kms = backend.kms().await;

    // Create a symmetric CMK. On real AWS this costs $1/month if not
    // cleaned up, so we schedule deletion in tearDown with the minimum
    // pending window (7 days).
    let key = kms
        .create_key()
        .description("fcparity test key")
        .send()
        .await
        .expect("create_key");
    let metadata = key.key_metadata().expect("key_metadata");
    let key_id = metadata.key_id().to_string();
    let arn = metadata.arn().expect("arn").to_string();
    assert!(
        arn.starts_with("arn:aws:kms:"),
        "kms key arn should start with arn:aws:kms: ; got {arn}"
    );

    let plaintext = b"hello kms parity".to_vec();
    let enc = kms
        .encrypt()
        .key_id(&key_id)
        .plaintext(Blob::new(plaintext.clone()))
        .send()
        .await
        .expect("encrypt");
    let ciphertext = enc.ciphertext_blob().expect("ciphertext_blob").clone();
    assert!(
        ciphertext.as_ref() != plaintext,
        "ciphertext should differ from plaintext"
    );

    let dec = kms
        .decrypt()
        .ciphertext_blob(ciphertext)
        .key_id(&key_id)
        .send()
        .await
        .expect("decrypt");
    let decrypted = dec.plaintext().expect("plaintext").clone();
    assert_eq!(decrypted.as_ref(), plaintext.as_slice());

    // Teardown: 7 days is the minimum pending window on real AWS.
    // Fakecloud accepts the same call; it's idempotent in both.
    let _ = kms
        .schedule_key_deletion()
        .key_id(key_id)
        .pending_window_in_days(7)
        .send()
        .await;
}

#[tokio::test]
async fn kms_generate_data_key_roundtrip() {
    let backend = Backend::from_env().await;
    let kms = backend.kms().await;

    let key = kms
        .create_key()
        .description("fcparity data-key test")
        .send()
        .await
        .expect("create_key");
    let key_id = key
        .key_metadata()
        .expect("key_metadata")
        .key_id()
        .to_string();

    // Ask KMS for a 256-bit data key. We get back both the plaintext and
    // an opaque ciphertext blob. Decrypting the blob should return the
    // same plaintext bytes.
    let dk = kms
        .generate_data_key()
        .key_id(&key_id)
        .key_spec(aws_sdk_kms::types::DataKeySpec::Aes256)
        .send()
        .await
        .expect("generate_data_key");
    let plaintext = dk.plaintext().expect("plaintext").clone();
    let ciphertext = dk.ciphertext_blob().expect("ciphertext_blob").clone();
    assert_eq!(
        plaintext.as_ref().len(),
        32,
        "AES-256 data key should be 32 bytes"
    );
    assert_ne!(
        plaintext.as_ref(),
        ciphertext.as_ref(),
        "plaintext and ciphertext should differ"
    );

    let dec = kms
        .decrypt()
        .ciphertext_blob(ciphertext)
        .key_id(&key_id)
        .send()
        .await
        .expect("decrypt");
    assert_eq!(
        dec.plaintext().expect("decrypted plaintext").as_ref(),
        plaintext.as_ref(),
        "decrypted data key should match the original plaintext"
    );

    let _ = kms
        .schedule_key_deletion()
        .key_id(key_id)
        .pending_window_in_days(7)
        .send()
        .await;
}
