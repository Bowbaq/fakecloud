//! Shared parity harness.
//!
//! Each test in this crate reads `FAKECLOUD_PARITY_BACKEND` at runtime and
//! runs the same body against either a local fakecloud process or a real
//! AWS account. The parity signal comes from comparing the pass/fail status
//! of the two CI jobs that run this crate, NOT from diffing responses
//! inside the tests.
//!
//! Rule of thumb for writing tests:
//!   - fakecloud passes, AWS passes -> good.
//!   - AWS passes, fakecloud fails -> fakecloud bug.
//!   - AWS fails, fakecloud passes -> test bug (assumed something wrong
//!     about AWS).
//!   - both fail -> test bug.
//!
//! A test must therefore never assert something that can't be true on real
//! AWS (exact ARNs, exact error messages, account IDs).

#![allow(dead_code, unused_imports)]

use aws_config::BehaviorVersion;
use aws_types::region::Region;
use fakecloud_testkit::TestServer;

pub use fakecloud_testkit::unique_name;

/// Which backend the current test run is targeting.
pub enum Backend {
    /// Spawns a local fakecloud process for the duration of the test.
    Fakecloud(TestServer),
    /// Uses the ambient AWS credentials from the environment.
    /// `region` is honoured explicitly so tests are reproducible.
    RealAws { region: String },
}

impl Backend {
    /// Pick a backend based on `FAKECLOUD_PARITY_BACKEND`. Defaults to
    /// `fakecloud` so a bare `cargo test -p fakecloud-parity` Just Works
    /// offline. Setting the env var to `aws` runs against real AWS and
    /// hard-panics if credentials are missing (tests never silently skip).
    pub async fn from_env() -> Self {
        let raw = std::env::var("FAKECLOUD_PARITY_BACKEND").unwrap_or_else(|_| "fakecloud".into());
        match raw.as_str() {
            "fakecloud" => Self::Fakecloud(TestServer::start().await),
            "aws" => {
                Self::require_aws_creds();
                let region =
                    std::env::var("AWS_REGION").unwrap_or_else(|_| "us-east-1".to_string());
                Self::RealAws { region }
            }
            other => panic!(
                "unknown FAKECLOUD_PARITY_BACKEND={other:?} \
                 (expected `fakecloud` or `aws`)"
            ),
        }
    }

    fn require_aws_creds() {
        let have_keys = std::env::var("AWS_ACCESS_KEY_ID").is_ok()
            && std::env::var("AWS_SECRET_ACCESS_KEY").is_ok();
        let have_profile = std::env::var("AWS_PROFILE").is_ok();
        let have_web_identity = std::env::var("AWS_WEB_IDENTITY_TOKEN_FILE").is_ok()
            && std::env::var("AWS_ROLE_ARN").is_ok();
        if !(have_keys || have_profile || have_web_identity) {
            panic!(
                "FAKECLOUD_PARITY_BACKEND=aws but no AWS credentials found. \
                 Set AWS_ACCESS_KEY_ID+AWS_SECRET_ACCESS_KEY, AWS_PROFILE, or \
                 AWS_WEB_IDENTITY_TOKEN_FILE+AWS_ROLE_ARN before running."
            );
        }
    }

    /// Short label used in panic messages / logs to tell the two runs apart.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Fakecloud(_) => "fakecloud",
            Self::RealAws { .. } => "real-aws",
        }
    }

    /// Is this run against real AWS? Some cleanup paths need to know
    /// (e.g. schedule-key-deletion has a 7-day minimum on real AWS).
    pub fn is_real_aws(&self) -> bool {
        matches!(self, Self::RealAws { .. })
    }

    /// Build an `SdkConfig` targeted at the current backend.
    pub async fn sdk_config(&self) -> aws_config::SdkConfig {
        match self {
            Self::Fakecloud(server) => server.aws_config().await,
            Self::RealAws { region } => {
                aws_config::defaults(BehaviorVersion::latest())
                    .region(Region::new(region.clone()))
                    .load()
                    .await
            }
        }
    }

    pub async fn sqs(&self) -> aws_sdk_sqs::Client {
        aws_sdk_sqs::Client::new(&self.sdk_config().await)
    }

    pub async fn sns(&self) -> aws_sdk_sns::Client {
        aws_sdk_sns::Client::new(&self.sdk_config().await)
    }

    pub async fn s3(&self) -> aws_sdk_s3::Client {
        let cfg = self.sdk_config().await;
        // Path-style keeps fakecloud happy (single endpoint) and also works
        // on real AWS. No downside.
        let s3_cfg = aws_sdk_s3::config::Builder::from(&cfg)
            .force_path_style(true)
            .build();
        aws_sdk_s3::Client::from_conf(s3_cfg)
    }

    pub async fn dynamodb(&self) -> aws_sdk_dynamodb::Client {
        aws_sdk_dynamodb::Client::new(&self.sdk_config().await)
    }

    pub async fn kms(&self) -> aws_sdk_kms::Client {
        aws_sdk_kms::Client::new(&self.sdk_config().await)
    }

    pub async fn secretsmanager(&self) -> aws_sdk_secretsmanager::Client {
        aws_sdk_secretsmanager::Client::new(&self.sdk_config().await)
    }

    pub async fn sts(&self) -> aws_sdk_sts::Client {
        aws_sdk_sts::Client::new(&self.sdk_config().await)
    }
}

/// Retry a fallible async fn up to `attempts` times with a fixed delay
/// between tries. Needed because real AWS is eventually consistent for
/// some list-after-create flows (S3 list after bucket create, DynamoDB
/// describe during table transition, etc.). Fakecloud is immediately
/// consistent so retries never fire against it.
pub async fn retry<F, Fut, T, E>(attempts: usize, delay_ms: u64, mut f: F) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    let mut last: Option<E> = None;
    for _ in 0..attempts {
        match f().await {
            Ok(v) => return Ok(v),
            Err(e) => {
                last = Some(e);
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            }
        }
    }
    Err(last.expect("retry called with attempts=0"))
}
