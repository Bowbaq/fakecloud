use async_trait::async_trait;

use fakecloud_core::service::{AwsRequest, AwsResponse, AwsService, AwsServiceError};

use crate::state::SharedRdsState;

pub struct RdsService {
    state: SharedRdsState,
}

impl RdsService {
    pub fn new(state: SharedRdsState) -> Self {
        Self { state }
    }
}

#[async_trait]
impl AwsService for RdsService {
    fn service_name(&self) -> &str {
        "rds"
    }

    async fn handle(&self, request: AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let _ = &self.state;
        Err(AwsServiceError::action_not_implemented(
            self.service_name(),
            &request.action,
        ))
    }

    fn supported_actions(&self) -> &[&str] {
        &[]
    }
}
