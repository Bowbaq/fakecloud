//! One `#[tokio::test]` per allow-listed service. Each runs every
//! `TestAcc*` test matching the service's `run_regex` minus its deny-list.
//!
//! Skips cleanly (doesn't fail) if the `go` or `terraform` binaries are
//! missing.

use fakecloud_tfacc::{
    allowlist::{Service, SERVICES},
    setup_provider_source, toolchain_available, GoTestRunner, TestServer,
};

async fn run_service(name: &str) {
    if !toolchain_available() {
        eprintln!("[tfacc] go or terraform not installed, skipping service `{name}`");
        return;
    }
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
