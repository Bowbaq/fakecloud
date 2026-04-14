mod common;

use common::Backend;

#[tokio::test]
async fn sts_get_caller_identity_shape() {
    let backend = Backend::from_env().await;
    let sts = backend.sts().await;
    let id = sts
        .get_caller_identity()
        .send()
        .await
        .expect("get_caller_identity");

    let account = id.account().expect("account");
    assert!(
        account.len() == 12 && account.chars().all(|c| c.is_ascii_digit()),
        "account should be 12 digits; got {account:?}"
    );

    let arn = id.arn().expect("arn");
    assert!(
        arn.starts_with("arn:aws:"),
        "arn should start with arn:aws: ; got {arn}"
    );

    let user_id = id.user_id().expect("user_id");
    assert!(!user_id.is_empty(), "user_id should not be empty");
}
