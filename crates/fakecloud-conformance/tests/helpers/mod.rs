//! Thin newtype wrapper over `fakecloud_testkit::TestServer` that adds the
//! per-service SDK client factories conformance tests rely on. Mirrors the
//! `fakecloud-e2e` helper pattern so the two harnesses stay aligned.

#![allow(dead_code)]

pub struct TestServer(fakecloud_testkit::TestServer);

impl TestServer {
    pub async fn start() -> Self {
        Self(fakecloud_testkit::TestServer::start().await)
    }

    pub fn endpoint(&self) -> &str {
        self.0.endpoint()
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

    pub async fn sesv2_client(&self) -> aws_sdk_sesv2::Client {
        aws_sdk_sesv2::Client::new(&self.aws_config().await)
    }

    pub async fn cognito_client(&self) -> aws_sdk_cognitoidentityprovider::Client {
        aws_sdk_cognitoidentityprovider::Client::new(&self.aws_config().await)
    }

    pub async fn sfn_client(&self) -> aws_sdk_sfn::Client {
        aws_sdk_sfn::Client::new(&self.aws_config().await)
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
}
