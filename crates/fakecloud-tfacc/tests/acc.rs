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
async fn dynamodb_acceptance() {
    run_service("dynamodb").await;
}
