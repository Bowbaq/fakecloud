//! End-to-end tests for the minimal Organizations control plane
//! (Batch 1: CreateOrganization / DescribeOrganization / DeleteOrganization).
//!
//! Drives `aws-sdk-organizations` against a fakecloud server running in
//! `FAKECLOUD_IAM=strict` to prove the wire format matches and that the
//! service participates in multi-account dispatch correctly.

mod helpers;

use aws_credential_types::Credentials;
use aws_sdk_organizations::Client as OrgsClient;
use helpers::TestServer;

const ACCOUNT_A: &str = "111111111111";
const ACCOUNT_B: &str = "222222222222";

async fn start() -> TestServer {
    TestServer::start_with_env(&[
        ("FAKECLOUD_IAM", "strict"),
        ("FAKECLOUD_VERIFY_SIGV4", "true"),
        // Organizations is pure control plane; no container runtime
        // needed. Skip the reaper to keep CI fast and avoid flaky
        // docker-info probes on machines where the daemon is slow.
        ("FAKECLOUD_CONTAINER_CLI", "false"),
    ])
    .await
}

async fn config_with(server: &TestServer, akid: &str, secret: &str) -> aws_config::SdkConfig {
    aws_config::defaults(aws_config::BehaviorVersion::latest())
        .endpoint_url(server.endpoint())
        .region(aws_config::Region::new("us-east-1"))
        .credentials_provider(Credentials::new(akid, secret, None, None, "orgs-test"))
        .load()
        .await
}

#[tokio::test]
async fn create_and_describe_round_trip() {
    let server = start().await;
    let (akid, secret) = server.create_admin(ACCOUNT_A, "admin-a").await;
    let cfg = config_with(&server, &akid, &secret).await;
    let orgs = OrgsClient::new(&cfg);

    let created = orgs.create_organization().send().await.unwrap();
    let org = created.organization().unwrap();
    assert_eq!(org.master_account_id().unwrap(), ACCOUNT_A);
    assert_eq!(
        org.feature_set().unwrap(),
        &aws_sdk_organizations::types::OrganizationFeatureSet::All
    );
    assert!(org.id().unwrap().starts_with("o-"));

    let described = orgs.describe_organization().send().await.unwrap();
    let org2 = described.organization().unwrap();
    assert_eq!(org2.id(), org.id());
    assert_eq!(org2.master_account_id().unwrap(), ACCOUNT_A);
}

#[tokio::test]
async fn second_create_fails_with_already_in_org() {
    let server = start().await;
    let (akid, secret) = server.create_admin(ACCOUNT_A, "admin-a").await;
    let cfg = config_with(&server, &akid, &secret).await;
    let orgs = OrgsClient::new(&cfg);

    orgs.create_organization().send().await.unwrap();
    let err = orgs.create_organization().send().await.unwrap_err();
    let msg = format!("{err:?}");
    assert!(
        msg.contains("AlreadyInOrganizationException"),
        "expected AlreadyInOrganizationException, got: {msg}"
    );
}

#[tokio::test]
async fn describe_without_org_returns_not_in_use() {
    let server = start().await;
    let (akid, secret) = server.create_admin(ACCOUNT_A, "admin-a").await;
    let cfg = config_with(&server, &akid, &secret).await;
    let orgs = OrgsClient::new(&cfg);

    let err = orgs.describe_organization().send().await.unwrap_err();
    let msg = format!("{err:?}");
    assert!(
        msg.contains("AWSOrganizationsNotInUseException"),
        "expected AWSOrganizationsNotInUseException, got: {msg}"
    );
}

#[tokio::test]
async fn only_management_can_delete_organization() {
    let server = start().await;
    let (a_akid, a_secret) = server.create_admin(ACCOUNT_A, "admin-a").await;
    let (b_akid, b_secret) = server.create_admin(ACCOUNT_B, "admin-b").await;

    let a_cfg = config_with(&server, &a_akid, &a_secret).await;
    let b_cfg = config_with(&server, &b_akid, &b_secret).await;
    let orgs_a = OrgsClient::new(&a_cfg);
    let orgs_b = OrgsClient::new(&b_cfg);

    // Account A creates the organization -> A is the management account.
    orgs_a.create_organization().send().await.unwrap();

    // Account B is not a member of the organization, so both
    // DescribeOrganization and DeleteOrganization must look exactly
    // like "no org exists" — we don't leak org metadata to non-members.
    let err = orgs_b.describe_organization().send().await.unwrap_err();
    assert!(format!("{err:?}").contains("AWSOrganizationsNotInUseException"));
    let err = orgs_b.delete_organization().send().await.unwrap_err();
    let msg = format!("{err:?}");
    assert!(
        msg.contains("AWSOrganizationsNotInUseException"),
        "expected AWSOrganizationsNotInUseException, got: {msg}"
    );

    // Management account deletes successfully.
    orgs_a.delete_organization().send().await.unwrap();

    // Describe now fails again -> state really went back to None.
    let err = orgs_a.describe_organization().send().await.unwrap_err();
    assert!(format!("{err:?}").contains("AWSOrganizationsNotInUseException"));
}
