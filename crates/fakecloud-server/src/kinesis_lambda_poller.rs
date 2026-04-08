use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use chrono::Utc;
use serde_json::json;

use fakecloud_core::delivery::LambdaDelivery;
use fakecloud_kinesis::state::SharedKinesisState;
use fakecloud_lambda::state::{LambdaInvocation, SharedLambdaState};

pub struct KinesisLambdaPoller {
    kinesis_state: SharedKinesisState,
    lambda_state: SharedLambdaState,
    lambda_delivery: Option<Arc<dyn LambdaDelivery>>,
}

impl KinesisLambdaPoller {
    pub fn new(kinesis_state: SharedKinesisState, lambda_state: SharedLambdaState) -> Self {
        Self {
            kinesis_state,
            lambda_state,
            lambda_delivery: None,
        }
    }

    pub fn with_lambda_delivery(mut self, delivery: Arc<dyn LambdaDelivery>) -> Self {
        self.lambda_delivery = Some(delivery);
        self
    }

    pub async fn run(self) {
        let mut interval = tokio::time::interval(Duration::from_millis(500));
        loop {
            interval.tick().await;
            self.poll().await;
        }
    }

    async fn poll(&self) {
        let mappings: Vec<(String, String, String, i64)> = {
            let lambda = self.lambda_state.read();
            lambda
                .event_source_mappings
                .values()
                .filter(|m| m.enabled && m.event_source_arn.contains(":kinesis:"))
                .map(|m| {
                    (
                        m.uuid.clone(),
                        m.event_source_arn.clone(),
                        m.function_arn.clone(),
                        m.batch_size,
                    )
                })
                .collect()
        };

        if mappings.is_empty() {
            return;
        }

        for (mapping_uuid, stream_arn, function_arn, batch_size) in mappings {
            let deliveries = {
                let kinesis = self.kinesis_state.read();
                let stream = match kinesis
                    .streams
                    .values()
                    .find(|s| s.stream_arn == stream_arn)
                {
                    Some(stream) => stream,
                    None => continue,
                };

                let limit = batch_size.max(1) as usize;
                stream
                    .shards
                    .iter()
                    .filter_map(|shard| {
                        let start = kinesis.lambda_checkpoint(&mapping_uuid, &shard.shard_id);
                        if start >= shard.records.len() {
                            return None;
                        }
                        let end = shard.records.len().min(start.saturating_add(limit));
                        let records = shard.records[start..end].to_vec();
                        Some((shard.shard_id.clone(), start, end, records))
                    })
                    .collect::<Vec<_>>()
            };

            for (shard_id, _start, end, records) in deliveries {
                let payload = json!({
                    "Records": records
                        .iter()
                        .map(|record| {
                            json!({
                                "awsRegion": "us-east-1",
                                "eventID": format!("{}:{}", shard_id, record.sequence_number),
                                "eventName": "aws:kinesis:record",
                                "eventSource": "aws:kinesis",
                                "eventSourceARN": stream_arn,
                                "eventVersion": "1.0",
                                "invokeIdentityArn": "arn:aws:iam::123456789012:role/lambda-role",
                                "kinesis": {
                                    "approximateArrivalTimestamp": record.approximate_arrival_timestamp.timestamp_millis() as f64 / 1000.0,
                                    "data": base64::engine::general_purpose::STANDARD.encode(&record.data),
                                    "kinesisSchemaVersion": "1.0",
                                    "partitionKey": record.partition_key,
                                    "sequenceNumber": record.sequence_number,
                                }
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .to_string();

                let used_real_delivery = self.lambda_delivery.is_some();
                let delivered = if let Some(ref delivery) = self.lambda_delivery {
                    match delivery.invoke_lambda(&function_arn, &payload).await {
                        Ok(_) => true,
                        Err(error) => {
                            tracing::warn!(
                                function_arn = %function_arn,
                                stream_arn = %stream_arn,
                                shard_id = %shard_id,
                                error = %error,
                                "Kinesis->Lambda: function invocation failed"
                            );
                            false
                        }
                    }
                } else {
                    true
                };

                if !delivered {
                    continue;
                }

                {
                    let mut kinesis = self.kinesis_state.write();
                    kinesis.set_lambda_checkpoint(&mapping_uuid, &shard_id, end);
                }

                if !used_real_delivery {
                    let mut lambda = self.lambda_state.write();
                    lambda.invocations.push(LambdaInvocation {
                        function_arn: function_arn.clone(),
                        payload,
                        timestamp: Utc::now(),
                        source: "aws:kinesis".to_string(),
                    });
                }
            }
        }
    }
}
