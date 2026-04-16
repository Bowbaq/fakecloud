//! End-to-end tests for ABAC (attribute-based access control) tag conditions.
//!
//! Tests `aws:PrincipalTag/<key>` condition evaluation with
//! `FAKECLOUD_IAM=strict`. The framework plumbs principal tags from
//! IamState through to the condition evaluator; this file proves it
//! works end-to-end with real aws-sdk-rust calls.

mod helpers;

use aws_credential_types::Credentials;
use aws_sdk_iam::Client as IamClient;
use aws_sdk_sts::Client as StsClient;
use helpers::TestServer;

async fn start_strict() -> TestServer {
    TestServer::start_with_env(&[
        ("FAKECLOUD_IAM", "strict"),
        ("FAKECLOUD_VERIFY_SIGV4", "true"),
    ])
    .await
}

async fn sdk_config_with(server: &TestServer, akid: &str, secret: &str) -> aws_config::SdkConfig {
    aws_config::defaults(aws_config::BehaviorVersion::latest())
        .endpoint_url(server.endpoint())
        .region(aws_config::Region::new("us-east-1"))
        .credentials_provider(Credentials::new(akid, secret, None, None, "fakecloud-abac"))
        .load()
        .await
}

async fn bootstrap_tagged_user(
    server: &TestServer,
    name: &str,
    tags: &[(&str, &str)],
) -> (String, String) {
    let boot = sdk_config_with(server, "test", "test").await;
    let iam = IamClient::new(&boot);
    let mut req = iam.create_user().user_name(name);
    for (k, v) in tags {
        req = req.tags(
            aws_sdk_iam::types::Tag::builder()
                .key(*k)
                .value(*v)
                .build()
                .unwrap(),
        );
    }
    req.send().await.unwrap();
    let ak = iam
        .create_access_key()
        .user_name(name)
        .send()
        .await
        .unwrap();
    let key = ak.access_key().unwrap();
    (
        key.access_key_id().to_string(),
        key.secret_access_key().to_string(),
    )
}

async fn attach_inline_policy(server: &TestServer, user: &str, name: &str, document: &str) {
    let boot = sdk_config_with(server, "test", "test").await;
    IamClient::new(&boot)
        .put_user_policy()
        .user_name(user)
        .policy_name(name)
        .policy_document(document)
        .send()
        .await
        .unwrap();
}

// ======================================================================
// aws:PrincipalTag tests
// ======================================================================

#[tokio::test]
async fn principal_tag_condition_allows_matching_user() {
    let server = start_strict().await;
    let (akid, secret) = bootstrap_tagged_user(&server, "alice", &[("Team", "platform")]).await;

    // Policy: allow sts:GetCallerIdentity only if PrincipalTag/Team == platform
    attach_inline_policy(
        &server,
        "alice",
        "AllowIfPlatform",
        r#"{"Version":"2012-10-17","Statement":[{
            "Effect":"Allow",
            "Action":"sts:GetCallerIdentity",
            "Resource":"*",
            "Condition":{"StringEquals":{"aws:PrincipalTag/Team":"platform"}}
        }]}"#,
    )
    .await;

    let cfg = sdk_config_with(&server, &akid, &secret).await;
    let sts = StsClient::new(&cfg);
    let identity = sts.get_caller_identity().send().await.unwrap();
    assert!(identity.arn().unwrap().contains("user/alice"));
}

#[tokio::test]
async fn principal_tag_condition_denies_non_matching_user() {
    let server = start_strict().await;
    let (akid, secret) = bootstrap_tagged_user(&server, "bob", &[("Team", "backend")]).await;

    // Policy: allow sts:GetCallerIdentity only if PrincipalTag/Team == platform
    attach_inline_policy(
        &server,
        "bob",
        "AllowIfPlatform",
        r#"{"Version":"2012-10-17","Statement":[{
            "Effect":"Allow",
            "Action":"sts:GetCallerIdentity",
            "Resource":"*",
            "Condition":{"StringEquals":{"aws:PrincipalTag/Team":"platform"}}
        }]}"#,
    )
    .await;

    let cfg = sdk_config_with(&server, &akid, &secret).await;
    let sts = StsClient::new(&cfg);
    let err = sts.get_caller_identity().send().await.unwrap_err();
    assert!(
        format!("{err:?}").contains("AccessDeniedException"),
        "expected AccessDeniedException"
    );
}

#[tokio::test]
async fn principal_tag_condition_denies_user_with_no_tags() {
    let server = start_strict().await;
    let (akid, secret) = bootstrap_tagged_user(&server, "carol", &[]).await;

    // Policy requires PrincipalTag/Team == platform, but carol has no tags
    attach_inline_policy(
        &server,
        "carol",
        "AllowIfPlatform",
        r#"{"Version":"2012-10-17","Statement":[{
            "Effect":"Allow",
            "Action":"sts:GetCallerIdentity",
            "Resource":"*",
            "Condition":{"StringEquals":{"aws:PrincipalTag/Team":"platform"}}
        }]}"#,
    )
    .await;

    let cfg = sdk_config_with(&server, &akid, &secret).await;
    let sts = StsClient::new(&cfg);
    let err = sts.get_caller_identity().send().await.unwrap_err();
    assert!(
        format!("{err:?}").contains("AccessDeniedException"),
        "expected AccessDeniedException"
    );
}

#[tokio::test]
async fn principal_tag_case_sensitive_key() {
    let server = start_strict().await;
    // User has tag "team" (lowercase), but policy checks "Team" (titlecase)
    let (akid, secret) = bootstrap_tagged_user(&server, "dave", &[("team", "platform")]).await;

    attach_inline_policy(
        &server,
        "dave",
        "AllowIfPlatform",
        r#"{"Version":"2012-10-17","Statement":[{
            "Effect":"Allow",
            "Action":"sts:GetCallerIdentity",
            "Resource":"*",
            "Condition":{"StringEquals":{"aws:PrincipalTag/Team":"platform"}}
        }]}"#,
    )
    .await;

    let cfg = sdk_config_with(&server, &akid, &secret).await;
    let sts = StsClient::new(&cfg);
    // Tag key mismatch: "team" != "Team" -> condition fails -> denied
    let err = sts.get_caller_identity().send().await.unwrap_err();
    assert!(
        format!("{err:?}").contains("AccessDeniedException"),
        "expected AccessDeniedException — tag keys are case-sensitive"
    );
}
