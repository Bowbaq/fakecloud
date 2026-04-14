mod common;

use common::{unique_name, Backend};

#[tokio::test]
async fn secretsmanager_create_get_put_delete() {
    let backend = Backend::from_env().await;
    let sm = backend.secretsmanager().await;
    let name = unique_name("sec");

    let create = sm
        .create_secret()
        .name(&name)
        .secret_string("v1")
        .send()
        .await
        .expect("create_secret");
    let arn = create.arn().expect("arn").to_string();
    assert!(
        arn.starts_with("arn:aws:secretsmanager:"),
        "secret arn should start with arn:aws:secretsmanager: ; got {arn}"
    );

    let got = sm
        .get_secret_value()
        .secret_id(&name)
        .send()
        .await
        .expect("get_secret_value");
    assert_eq!(got.secret_string(), Some("v1"));

    // Put a new version.
    sm.put_secret_value()
        .secret_id(&name)
        .secret_string("v2")
        .send()
        .await
        .expect("put_secret_value");

    let got2 = sm
        .get_secret_value()
        .secret_id(&name)
        .send()
        .await
        .expect("get_secret_value v2");
    assert_eq!(got2.secret_string(), Some("v2"));

    // Force delete so real AWS cleans up immediately instead of the
    // default 7-day recovery window.
    sm.delete_secret()
        .secret_id(&name)
        .force_delete_without_recovery(true)
        .send()
        .await
        .expect("delete_secret");
}
