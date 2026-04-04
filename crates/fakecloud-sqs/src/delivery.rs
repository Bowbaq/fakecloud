use std::collections::HashMap;

use chrono::Utc;

use fakecloud_core::delivery::SqsDelivery;

use crate::state::{SharedSqsState, SqsMessage};

/// Implements SqsDelivery so other services can push messages into SQS queues.
pub struct SqsDeliveryImpl {
    state: SharedSqsState,
}

impl SqsDeliveryImpl {
    pub fn new(state: SharedSqsState) -> Self {
        Self { state }
    }
}

impl SqsDelivery for SqsDeliveryImpl {
    fn deliver_to_queue(
        &self,
        queue_arn: &str,
        message_body: &str,
        _attributes: &HashMap<String, String>,
    ) {
        let mut state = self.state.write();

        // Find queue by ARN
        let queue = state.queues.values_mut().find(|q| q.arn == queue_arn);

        if let Some(queue) = queue {
            let now = Utc::now();
            let msg = SqsMessage {
                message_id: uuid::Uuid::new_v4().to_string(),
                receipt_handle: None,
                md5_of_body: crate::service::md5_hex(message_body),
                body: message_body.to_string(),
                sent_timestamp: now.timestamp_millis(),
                attributes: HashMap::new(),
                message_attributes: HashMap::new(),
                visible_at: None,
                receive_count: 0,
                message_group_id: None,
                message_dedup_id: None,
                created_at: now,
            };
            queue.messages.push_back(msg);
            tracing::debug!(queue_arn, "delivered message to SQS queue");
        } else {
            tracing::warn!(queue_arn, "SQS delivery target queue not found");
        }
    }
}
