mod helpers;

use aws_sdk_ssm::types::{DocumentFormat, DocumentType, ParameterType};
use helpers::TestServer;

/// Parameter (with tags, description, version history) survives restart.
#[tokio::test]
async fn persistence_round_trip_parameter_with_history_and_tags() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let ssm = server.ssm_client().await;

    ssm.put_parameter()
        .name("/app/db/url")
        .value("postgres://v1")
        .r#type(ParameterType::String)
        .description("app db url")
        .send()
        .await
        .unwrap();

    ssm.add_tags_to_resource()
        .resource_type("Parameter".into())
        .resource_id("/app/db/url")
        .tags(
            aws_sdk_ssm::types::Tag::builder()
                .key("env")
                .value("prod")
                .build()
                .unwrap(),
        )
        .send()
        .await
        .unwrap();

    // New version.
    ssm.put_parameter()
        .name("/app/db/url")
        .value("postgres://v2")
        .r#type(ParameterType::String)
        .overwrite(true)
        .send()
        .await
        .unwrap();

    server.restart().await;
    let ssm = server.ssm_client().await;

    let got = ssm
        .get_parameter()
        .name("/app/db/url")
        .send()
        .await
        .unwrap();
    let p = got.parameter().unwrap();
    assert_eq!(p.value(), Some("postgres://v2"));
    assert_eq!(p.version(), 2);

    let history = ssm
        .get_parameter_history()
        .name("/app/db/url")
        .send()
        .await
        .unwrap();
    assert!(history.parameters().len() >= 2);

    let tags = ssm
        .list_tags_for_resource()
        .resource_type("Parameter".into())
        .resource_id("/app/db/url")
        .send()
        .await
        .unwrap();
    assert!(tags
        .tag_list()
        .iter()
        .any(|t| t.key() == "env" && t.value() == "prod"));
}

/// SecureString parameter and DeleteParameter survive restart.
#[tokio::test]
async fn persistence_secure_string_and_delete_survive_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let ssm = server.ssm_client().await;

    ssm.put_parameter()
        .name("/secret/token")
        .value("s3cret")
        .r#type(ParameterType::SecureString)
        .send()
        .await
        .unwrap();
    ssm.put_parameter()
        .name("/will/be/deleted")
        .value("ephemeral")
        .r#type(ParameterType::String)
        .send()
        .await
        .unwrap();
    ssm.delete_parameter()
        .name("/will/be/deleted")
        .send()
        .await
        .unwrap();

    server.restart().await;
    let ssm = server.ssm_client().await;

    let got = ssm
        .get_parameter()
        .name("/secret/token")
        .with_decryption(true)
        .send()
        .await
        .unwrap();
    let p = got.parameter().unwrap();
    assert_eq!(p.value(), Some("s3cret"));
    assert_eq!(p.r#type(), Some(&ParameterType::SecureString));

    // Deleted parameter stays deleted.
    let err = ssm
        .get_parameter()
        .name("/will/be/deleted")
        .send()
        .await
        .err();
    assert!(err.is_some());
}

/// Custom SSM document survives restart.
#[tokio::test]
async fn persistence_document_survives_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let ssm = server.ssm_client().await;

    let content = r#"{"schemaVersion":"2.2","description":"noop","mainSteps":[]}"#;
    ssm.create_document()
        .name("NoopDoc")
        .content(content)
        .document_type(DocumentType::Command)
        .document_format(DocumentFormat::Json)
        .send()
        .await
        .unwrap();

    server.restart().await;
    let ssm = server.ssm_client().await;

    let got = ssm.get_document().name("NoopDoc").send().await.unwrap();
    assert_eq!(got.name(), Some("NoopDoc"));
    assert_eq!(got.content(), Some(content));
}
