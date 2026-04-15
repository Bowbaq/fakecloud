//! One `#[tokio::test]` per allow-listed service. Each runs every
//! `TestAcc*` test matching the service's `run_regex` minus its deny-list.
//!
//! Hard-fails if the `go` or `terraform` binaries are missing. Running
//! this crate is an opt-in signal that the caller wants the upstream
//! Terraform suite exercised — silently passing on a machine that can't
//! run it would just hide regressions.

use fakecloud_tfacc::{
    allowlist::{Service, SERVICES},
    require_toolchain, setup_provider_source, GoTestRunner, TestServer,
};

async fn run_service(name: &str) {
    require_toolchain();
    let service: &Service = SERVICES
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("service `{name}` not in SERVICES allow-list"));

    let provider_root = setup_provider_source().expect("setup terraform-provider-aws");
    let server = TestServer::start().await;
    let runner = GoTestRunner {
        provider_root: &provider_root,
        endpoint: server.endpoint(),
    };
    runner.run_service(service).assert_pass(name);
}

#[tokio::test]
async fn cognitoidp_acceptance() {
    run_service("cognitoidp").await;
}

#[tokio::test]
async fn bedrock_acceptance() {
    run_service("bedrock").await;
}

#[tokio::test]
async fn apigatewayv2_acceptance() {
    run_service("apigatewayv2").await;
}

#[tokio::test]
async fn kinesis_acceptance() {
    run_service("kinesis").await;
}

#[tokio::test]
async fn sns_acceptance() {
    run_service("sns").await;
}

#[tokio::test]
async fn events_acceptance() {
    run_service("events").await;
}

#[tokio::test]
async fn kms_acceptance() {
    run_service("kms").await;
}

#[tokio::test]
async fn logs_acceptance() {
    run_service("logs").await;
}

#[tokio::test]
async fn iam_acceptance() {
    run_service("iam").await;
}

#[tokio::test]
async fn ssm_acceptance() {
    run_service("ssm").await;
}

#[tokio::test]
async fn secretsmanager_acceptance() {
    run_service("secretsmanager").await;
}

#[tokio::test]
async fn sqs_acceptance() {
    run_service("sqs").await;
}

#[tokio::test]
async fn dynamodb_acceptance() {
    run_service("dynamodb").await;
}
