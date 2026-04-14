//! Thin newtype wrapper over `fakecloud_testkit::TestServer` that adds the
//! per-service SDK client factories e2e tests rely on. Keeps testkit itself
//! free of the `aws-sdk-*` dependency sprawl while preserving the
//! `helpers::TestServer` API existing e2e tests already use.

#![allow(dead_code, unused_imports)]

use std::path::{Path, PathBuf};
use std::process::Command;

pub use fakecloud_testkit::{data_path_for, run_until_exit, CliOutput};

/// Newtype wrapper around `fakecloud_testkit::TestServer`. Delegates all
/// lifecycle methods and layers on per-service AWS SDK client factories.
pub struct TestServer(fakecloud_testkit::TestServer);

impl TestServer {
    pub async fn start() -> Self {
        Self(fakecloud_testkit::TestServer::start().await)
    }

    pub async fn start_with_env(env: &[(&str, &str)]) -> Self {
        Self(fakecloud_testkit::TestServer::start_with_env(env).await)
    }

    pub async fn start_full(env: &[(&str, &str)], extra_args: &[&str]) -> Self {
        Self(fakecloud_testkit::TestServer::start_full(env, extra_args).await)
    }

    pub async fn start_persistent(data_path: &Path) -> Self {
        Self(fakecloud_testkit::TestServer::start_persistent(data_path).await)
    }

    pub async fn start_persistent_with_cache(data_path: &Path, s3_cache_size: Option<u64>) -> Self {
        Self(
            fakecloud_testkit::TestServer::start_persistent_with_cache(data_path, s3_cache_size)
                .await,
        )
    }

    pub async fn restart(&mut self) {
        self.0.restart().await
    }

    pub fn endpoint(&self) -> &str {
        self.0.endpoint()
    }

    pub fn port(&self) -> u16 {
        self.0.port()
    }

    pub async fn aws_config(&self) -> aws_config::SdkConfig {
        self.0.aws_config().await
    }

    pub async fn sqs_client(&self) -> aws_sdk_sqs::Client {
        aws_sdk_sqs::Client::new(&self.aws_config().await)
    }

    pub async fn sns_client(&self) -> aws_sdk_sns::Client {
        aws_sdk_sns::Client::new(&self.aws_config().await)
    }

    pub async fn eventbridge_client(&self) -> aws_sdk_eventbridge::Client {
        aws_sdk_eventbridge::Client::new(&self.aws_config().await)
    }

    pub async fn iam_client(&self) -> aws_sdk_iam::Client {
        aws_sdk_iam::Client::new(&self.aws_config().await)
    }

    pub async fn sts_client(&self) -> aws_sdk_sts::Client {
        aws_sdk_sts::Client::new(&self.aws_config().await)
    }

    pub async fn ssm_client(&self) -> aws_sdk_ssm::Client {
        aws_sdk_ssm::Client::new(&self.aws_config().await)
    }

    pub async fn dynamodb_client(&self) -> aws_sdk_dynamodb::Client {
        aws_sdk_dynamodb::Client::new(&self.aws_config().await)
    }

    pub async fn lambda_client(&self) -> aws_sdk_lambda::Client {
        aws_sdk_lambda::Client::new(&self.aws_config().await)
    }

    pub async fn secretsmanager_client(&self) -> aws_sdk_secretsmanager::Client {
        aws_sdk_secretsmanager::Client::new(&self.aws_config().await)
    }

    pub async fn logs_client(&self) -> aws_sdk_cloudwatchlogs::Client {
        aws_sdk_cloudwatchlogs::Client::new(&self.aws_config().await)
    }

    pub async fn kms_client(&self) -> aws_sdk_kms::Client {
        aws_sdk_kms::Client::new(&self.aws_config().await)
    }

    pub async fn kinesis_client(&self) -> aws_sdk_kinesis::Client {
        aws_sdk_kinesis::Client::new(&self.aws_config().await)
    }

    pub async fn rds_client(&self) -> aws_sdk_rds::Client {
        aws_sdk_rds::Client::new(&self.aws_config().await)
    }

    pub async fn elasticache_client(&self) -> aws_sdk_elasticache::Client {
        aws_sdk_elasticache::Client::new(&self.aws_config().await)
    }

    pub async fn cloudformation_client(&self) -> aws_sdk_cloudformation::Client {
        aws_sdk_cloudformation::Client::new(&self.aws_config().await)
    }

    pub async fn ses_client(&self) -> aws_sdk_ses::Client {
        aws_sdk_ses::Client::new(&self.aws_config().await)
    }

    pub async fn sesv2_client(&self) -> aws_sdk_sesv2::Client {
        aws_sdk_sesv2::Client::new(&self.aws_config().await)
    }

    pub async fn cognito_client(&self) -> aws_sdk_cognitoidentityprovider::Client {
        aws_sdk_cognitoidentityprovider::Client::new(&self.aws_config().await)
    }

    pub async fn sfn_client(&self) -> aws_sdk_sfn::Client {
        aws_sdk_sfn::Client::new(&self.aws_config().await)
    }

    pub async fn apigatewayv2_client(&self) -> aws_sdk_apigatewayv2::Client {
        aws_sdk_apigatewayv2::Client::new(&self.aws_config().await)
    }

    pub async fn bedrock_client(&self) -> aws_sdk_bedrock::Client {
        aws_sdk_bedrock::Client::new(&self.aws_config().await)
    }

    pub async fn bedrock_runtime_client(&self) -> aws_sdk_bedrockruntime::Client {
        aws_sdk_bedrockruntime::Client::new(&self.aws_config().await)
    }

    pub async fn s3_client(&self) -> aws_sdk_s3::Client {
        let config = self.aws_config().await;
        let s3_config = aws_sdk_s3::config::Builder::from(&config)
            .force_path_style(true)
            .build();
        aws_sdk_s3::Client::from_conf(s3_config)
    }

    pub async fn aws_cli(&self, args: &[&str]) -> CliOutput {
        let output = Command::new("aws")
            .args(args)
            .arg("--endpoint-url")
            .arg(self.endpoint())
            .arg("--region")
            .arg("us-east-1")
            .env("AWS_ACCESS_KEY_ID", "test")
            .env("AWS_SECRET_ACCESS_KEY", "test")
            .env("AWS_DEFAULT_REGION", "us-east-1")
            .output()
            .expect("failed to run aws cli");
        CliOutput(output)
    }
}

/// Decompress gzipped data.
pub fn gunzip(data: &[u8]) -> Vec<u8> {
    use std::io::Read;
    let mut decoder = flate2::read::GzDecoder::new(data);
    let mut result = Vec::new();
    decoder.read_to_end(&mut result).unwrap();
    result
}

/// Re-exported for historical reasons. Some test helpers construct a
/// `PathBuf` from a `tempfile::TempDir` through this shim.
#[allow(dead_code)]
pub fn _path_buf_shim(p: PathBuf) -> PathBuf {
    p
}
