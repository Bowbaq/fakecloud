//! End-to-end tests for opt-in IAM identity-policy enforcement on IAM + STS.
//!
//! Each test spawns fakecloud with `FAKECLOUD_IAM=strict`, bootstraps a
//! user via the reserved `test*` root-bypass credentials, attaches an
//! inline policy, then signs a follow-up request with the user's own
//! access key to observe Allow / Deny. Batch 7 wires IAM + STS first;
//! batches 7b/8 roll out the rest.

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

async fn start_soft() -> TestServer {
    TestServer::start_with_env(&[("FAKECLOUD_IAM", "soft")]).await
}

async fn sdk_config_with(server: &TestServer, akid: &str, secret: &str) -> aws_config::SdkConfig {
    aws_config::defaults(aws_config::BehaviorVersion::latest())
        .endpoint_url(server.endpoint())
        .region(aws_config::Region::new("us-east-1"))
        .credentials_provider(Credentials::new(
            akid,
            secret,
            None,
            None,
            "fakecloud-iam-enf",
        ))
        .load()
        .await
}

/// Bootstrap a user via root-bypass credentials and return their
/// freshly-created (access_key_id, secret_access_key) plus an
/// `aws-sdk-iam` client signed with those credentials.
async fn bootstrap_user(server: &TestServer, name: &str) -> (String, String) {
    let boot = sdk_config_with(server, "test", "test").await;
    let iam_boot = IamClient::new(&boot);
    iam_boot.create_user().user_name(name).send().await.unwrap();
    let ak = iam_boot
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
// STS tests
// ======================================================================

#[tokio::test]
async fn sts_get_caller_identity_denied_without_policy() {
    // A user with no attached policies has implicit-deny on STS actions
    // in strict mode. GetCallerIdentity with their credentials must fail.
    let server = start_strict().await;
    let (akid, secret) = bootstrap_user(&server, "alice").await;

    let cfg = sdk_config_with(&server, &akid, &secret).await;
    let sts = StsClient::new(&cfg);
    let err = sts.get_caller_identity().send().await.unwrap_err();
    let msg = format!("{err:?}");
    assert!(
        msg.contains("AccessDeniedException"),
        "expected AccessDeniedException, got {msg}"
    );
}

#[tokio::test]
async fn sts_get_caller_identity_allowed_with_explicit_policy() {
    let server = start_strict().await;
    let (akid, secret) = bootstrap_user(&server, "bob").await;
    attach_inline_policy(
        &server,
        "bob",
        "AllowGetCallerIdentity",
        r#"{"Version":"2012-10-17","Statement":[{"Effect":"Allow","Action":"sts:GetCallerIdentity","Resource":"*"}]}"#,
    )
    .await;

    let cfg = sdk_config_with(&server, &akid, &secret).await;
    let sts = StsClient::new(&cfg);
    let identity = sts.get_caller_identity().send().await.unwrap();
    assert!(identity.arn().unwrap().contains("user/bob"));
}

#[tokio::test]
async fn sts_explicit_deny_beats_allow_all() {
    let server = start_strict().await;
    let (akid, secret) = bootstrap_user(&server, "carol").await;
    attach_inline_policy(
        &server,
        "carol",
        "AllowAll",
        r#"{"Version":"2012-10-17","Statement":[{"Effect":"Allow","Action":"*","Resource":"*"}]}"#,
    )
    .await;
    attach_inline_policy(
        &server,
        "carol",
        "DenySts",
        r#"{"Version":"2012-10-17","Statement":[{"Effect":"Deny","Action":"sts:GetCallerIdentity","Resource":"*"}]}"#,
    )
    .await;

    let cfg = sdk_config_with(&server, &akid, &secret).await;
    let sts = StsClient::new(&cfg);
    let err = sts.get_caller_identity().send().await.unwrap_err();
    let msg = format!("{err:?}");
    assert!(msg.contains("AccessDeniedException"), "got {msg}");
}

#[tokio::test]
async fn root_bypass_skips_enforcement() {
    // The reserved `test`/`test` root bypass should succeed even under
    // strict enforcement with no explicit policy.
    let server = start_strict().await;
    let cfg = sdk_config_with(&server, "test", "test").await;
    let sts = StsClient::new(&cfg);
    let identity = sts.get_caller_identity().send().await.unwrap();
    assert!(identity.arn().unwrap().contains(":root"));
}

// ======================================================================
// IAM tests
// ======================================================================

#[tokio::test]
async fn iam_get_user_resource_scoped_policy() {
    // Bob can read his own user record but not alice's.
    let server = start_strict().await;
    // Create both users via root bypass.
    {
        let boot = sdk_config_with(&server, "test", "test").await;
        let boot_iam = IamClient::new(&boot);
        boot_iam
            .create_user()
            .user_name("alice")
            .send()
            .await
            .unwrap();
    }
    let (akid, secret) = bootstrap_user(&server, "bob").await;
    attach_inline_policy(
        &server,
        "bob",
        "ReadSelf",
        r#"{"Version":"2012-10-17","Statement":[{"Effect":"Allow","Action":"iam:GetUser","Resource":"arn:aws:iam::123456789012:user/bob"}]}"#,
    )
    .await;

    let cfg = sdk_config_with(&server, &akid, &secret).await;
    let iam = IamClient::new(&cfg);

    // Bob -> self: allowed.
    let self_resp = iam.get_user().user_name("bob").send().await.unwrap();
    assert_eq!(self_resp.user().unwrap().user_name(), "bob");

    // Bob -> alice: denied because resource doesn't match.
    let err = iam.get_user().user_name("alice").send().await.unwrap_err();
    assert!(format!("{err:?}").contains("AccessDeniedException"));
}

#[tokio::test]
async fn iam_wildcard_action_allows_everything() {
    let server = start_strict().await;
    let (akid, secret) = bootstrap_user(&server, "dave").await;
    attach_inline_policy(
        &server,
        "dave",
        "AllowAllIam",
        r#"{"Version":"2012-10-17","Statement":[{"Effect":"Allow","Action":"iam:*","Resource":"*"}]}"#,
    )
    .await;

    let cfg = sdk_config_with(&server, &akid, &secret).await;
    let iam = IamClient::new(&cfg);
    iam.list_users().send().await.unwrap();
    iam.get_user().user_name("dave").send().await.unwrap();
}

#[tokio::test]
async fn iam_soft_mode_does_not_fail_denied_requests() {
    // Soft mode should log the deny but let the request through.
    let server = start_soft().await;
    let (akid, secret) = bootstrap_user(&server, "erin").await;
    // No policies attached -> implicit deny in the evaluator, but soft
    // mode lets it through.
    let cfg = sdk_config_with(&server, &akid, &secret).await;
    let iam = IamClient::new(&cfg);
    // This would AccessDeny under strict mode; in soft mode it succeeds.
    iam.list_users().send().await.unwrap();
}

#[tokio::test]
async fn off_mode_does_not_enforce() {
    // Regression guard for off-by-default: no FAKECLOUD_IAM env set.
    let server = TestServer::start().await;
    let (akid, secret) = bootstrap_user(&server, "frank").await;
    let cfg = sdk_config_with(&server, &akid, &secret).await;
    let iam = IamClient::new(&cfg);
    // Frank has no policies, but enforcement is off -> succeeds.
    iam.list_users().send().await.unwrap();
}
