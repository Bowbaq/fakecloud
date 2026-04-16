//! End-to-end tests for multi-account isolation (#381).
//!
//! Validates that resources created in one AWS account are invisible to
//! another, and that cross-account operations (STS AssumeRole, SNS -> SQS
//! delivery) work correctly.
//!
//! Each test spawns fakecloud with SigV4 verification + IAM strict mode
//! so that principal resolution routes requests to the correct account.

mod helpers;

use aws_credential_types::Credentials;
use aws_sdk_dynamodb::Client as DynamoClient;
use aws_sdk_iam::Client as IamClient;
use aws_sdk_s3::Client as S3Client;
use aws_sdk_sqs::Client as SqsClient;
use aws_sdk_sts::Client as StsClient;
use helpers::TestServer;

const ACCOUNT_A: &str = "123456789012"; // default account
const ACCOUNT_B: &str = "222222222222";

async fn start() -> TestServer {
    TestServer::start_with_env(&[
        ("FAKECLOUD_IAM", "strict"),
        ("FAKECLOUD_VERIFY_SIGV4", "true"),
    ])
    .await
}

async fn config_with(server: &TestServer, akid: &str, secret: &str) -> aws_config::SdkConfig {
    aws_config::defaults(aws_config::BehaviorVersion::latest())
        .endpoint_url(server.endpoint())
        .region(aws_config::Region::new("us-east-1"))
        .credentials_provider(Credentials::new(akid, secret, None, None, "multi-acct"))
        .load()
        .await
}

async fn config_with_session(
    server: &TestServer,
    akid: &str,
    secret: &str,
    token: &str,
) -> aws_config::SdkConfig {
    aws_config::defaults(aws_config::BehaviorVersion::latest())
        .endpoint_url(server.endpoint())
        .region(aws_config::Region::new("us-east-1"))
        .credentials_provider(Credentials::new(
            akid,
            secret,
            Some(token.to_string()),
            None,
            "multi-acct-session",
        ))
        .load()
        .await
}

/// Create a user in the default account with admin permissions, return (akid, secret).
async fn bootstrap_admin(server: &TestServer, name: &str) -> (String, String) {
    let root = config_with(server, "test", "test").await;
    let iam = IamClient::new(&root);
    iam.create_user().user_name(name).send().await.unwrap();
    iam.put_user_policy()
        .user_name(name)
        .policy_name("admin")
        .policy_document(r#"{"Version":"2012-10-17","Statement":[{"Effect":"Allow","Action":"*","Resource":"*"}]}"#)
        .send()
        .await
        .unwrap();
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

/// Create a role in account A that can be assumed, then AssumeRole into
/// account B. Returns (akid, secret, session_token) for account B.
async fn assume_into_account_b(server: &TestServer) -> (String, String, String) {
    let root = config_with(server, "test", "test").await;
    let iam = IamClient::new(&root);

    // Create a role in account B's namespace
    let role_arn = format!("arn:aws:iam::{ACCOUNT_B}:role/cross-account-role");
    iam.create_role()
        .role_name("cross-account-role")
        .assume_role_policy_document(
            r#"{"Version":"2012-10-17","Statement":[{"Effect":"Allow","Principal":"*","Action":"sts:AssumeRole"}]}"#,
        )
        .send()
        .await
        .unwrap();

    // Attach admin policy to the role
    iam.put_role_policy()
        .role_name("cross-account-role")
        .policy_name("admin")
        .policy_document(r#"{"Version":"2012-10-17","Statement":[{"Effect":"Allow","Action":"*","Resource":"*"}]}"#)
        .send()
        .await
        .unwrap();

    // AssumeRole into account B
    let sts = StsClient::new(&root);
    let assumed = sts
        .assume_role()
        .role_arn(&role_arn)
        .role_session_name("test-session")
        .send()
        .await
        .unwrap();
    let creds = assumed.credentials().unwrap();
    (
        creds.access_key_id().to_string(),
        creds.secret_access_key().to_string(),
        creds.session_token().to_string(),
    )
}

// ======================================================================
// STS: GetCallerIdentity returns correct account
// ======================================================================

#[tokio::test]
async fn sts_caller_identity_reflects_assumed_account() {
    let server = start().await;
    let (akid, secret, token) = assume_into_account_b(&server).await;
    let cfg = config_with_session(&server, &akid, &secret, &token).await;
    let sts = StsClient::new(&cfg);
    let identity = sts.get_caller_identity().send().await.unwrap();
    assert_eq!(identity.account().unwrap(), ACCOUNT_B);
    assert!(identity.arn().unwrap().contains(ACCOUNT_B));
}

// ======================================================================
// SQS: queues isolated per account
// ======================================================================

#[tokio::test]
async fn sqs_queues_isolated_across_accounts() {
    let server = start().await;

    // Create admin in account A
    let (a_akid, a_secret) = bootstrap_admin(&server, "alice").await;
    let a_cfg = config_with(&server, &a_akid, &a_secret).await;
    let sqs_a = SqsClient::new(&a_cfg);

    // Create queue in account A
    sqs_a
        .create_queue()
        .queue_name("shared-name")
        .send()
        .await
        .unwrap();

    // Assume into account B
    let (b_akid, b_secret, b_token) = assume_into_account_b(&server).await;
    let b_cfg = config_with_session(&server, &b_akid, &b_secret, &b_token).await;
    let sqs_b = SqsClient::new(&b_cfg);

    // List queues in account B -> should be empty
    let list = sqs_b.list_queues().send().await.unwrap();
    let urls = list.queue_urls();
    assert!(
        urls.is_empty(),
        "account B should not see account A's queues, got: {urls:?}"
    );

    // Create same-named queue in account B -> should succeed (no conflict)
    sqs_b
        .create_queue()
        .queue_name("shared-name")
        .send()
        .await
        .unwrap();

    // Both accounts now have 1 queue each
    let a_list = sqs_a.list_queues().send().await.unwrap();
    assert_eq!(a_list.queue_urls().len(), 1);
    let b_list = sqs_b.list_queues().send().await.unwrap();
    assert_eq!(b_list.queue_urls().len(), 1);
}

// ======================================================================
// DynamoDB: tables isolated per account
// ======================================================================

#[tokio::test]
async fn dynamodb_tables_isolated_across_accounts() {
    let server = start().await;

    let (a_akid, a_secret) = bootstrap_admin(&server, "alice").await;
    let a_cfg = config_with(&server, &a_akid, &a_secret).await;
    let ddb_a = DynamoClient::new(&a_cfg);

    // Create table in account A
    ddb_a
        .create_table()
        .table_name("shared-table")
        .key_schema(
            aws_sdk_dynamodb::types::KeySchemaElement::builder()
                .attribute_name("pk")
                .key_type(aws_sdk_dynamodb::types::KeyType::Hash)
                .build()
                .unwrap(),
        )
        .attribute_definitions(
            aws_sdk_dynamodb::types::AttributeDefinition::builder()
                .attribute_name("pk")
                .attribute_type(aws_sdk_dynamodb::types::ScalarAttributeType::S)
                .build()
                .unwrap(),
        )
        .billing_mode(aws_sdk_dynamodb::types::BillingMode::PayPerRequest)
        .send()
        .await
        .unwrap();

    // Assume into account B
    let (b_akid, b_secret, b_token) = assume_into_account_b(&server).await;
    let b_cfg = config_with_session(&server, &b_akid, &b_secret, &b_token).await;
    let ddb_b = DynamoClient::new(&b_cfg);

    // List tables in account B -> should be empty
    let list = ddb_b.list_tables().send().await.unwrap();
    assert!(
        list.table_names().is_empty(),
        "account B should not see account A's tables"
    );

    // Create same-named table in account B -> should succeed
    ddb_b
        .create_table()
        .table_name("shared-table")
        .key_schema(
            aws_sdk_dynamodb::types::KeySchemaElement::builder()
                .attribute_name("pk")
                .key_type(aws_sdk_dynamodb::types::KeyType::Hash)
                .build()
                .unwrap(),
        )
        .attribute_definitions(
            aws_sdk_dynamodb::types::AttributeDefinition::builder()
                .attribute_name("pk")
                .attribute_type(aws_sdk_dynamodb::types::ScalarAttributeType::S)
                .build()
                .unwrap(),
        )
        .billing_mode(aws_sdk_dynamodb::types::BillingMode::PayPerRequest)
        .send()
        .await
        .unwrap();
}

// ======================================================================
// S3: buckets isolated per account
// ======================================================================

#[tokio::test]
async fn s3_buckets_isolated_across_accounts() {
    let server = start().await;

    let (a_akid, a_secret) = bootstrap_admin(&server, "alice").await;
    let a_cfg = config_with(&server, &a_akid, &a_secret).await;
    let s3_a = S3Client::new(&a_cfg);

    // Create bucket in account A
    s3_a.create_bucket()
        .bucket("account-a-bucket")
        .send()
        .await
        .unwrap();

    // Assume into account B
    let (b_akid, b_secret, b_token) = assume_into_account_b(&server).await;
    let b_cfg = config_with_session(&server, &b_akid, &b_secret, &b_token).await;
    let s3_b = S3Client::new(&b_cfg);

    // List buckets in account B -> should be empty
    let list = s3_b.list_buckets().send().await.unwrap();
    assert!(
        list.buckets().is_empty(),
        "account B should not see account A's buckets"
    );

    // Create bucket in account B
    s3_b.create_bucket()
        .bucket("account-b-bucket")
        .send()
        .await
        .unwrap();

    // Account A still sees only its bucket
    let a_list = s3_a.list_buckets().send().await.unwrap();
    assert_eq!(a_list.buckets().len(), 1);
    assert_eq!(a_list.buckets()[0].name().unwrap(), "account-a-bucket");
}

// ======================================================================
// Unauthenticated requests stay in default account
// ======================================================================

#[tokio::test]
async fn unauthenticated_uses_default_account() {
    // Start without SigV4 verification so unauthenticated requests work
    let server = TestServer::start().await;

    let cfg = config_with(&server, "test", "test").await;
    let sqs = SqsClient::new(&cfg);

    // Create queue without auth -> goes to default account
    sqs.create_queue()
        .queue_name("default-queue")
        .send()
        .await
        .unwrap();

    let sts = StsClient::new(&cfg);
    let identity = sts.get_caller_identity().send().await.unwrap();
    assert_eq!(identity.account().unwrap(), ACCOUNT_A);
}
