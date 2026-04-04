use async_trait::async_trait;
use chrono::Utc;
use http::StatusCode;
use md5::{Digest, Md5};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};

use fakecloud_core::service::{AwsRequest, AwsResponse, AwsService, AwsServiceError};

use crate::state::{MessageAttribute, RedrivePolicy, SharedSqsState, SqsMessage, SqsQueue};

pub struct SqsService {
    state: SharedSqsState,
}

impl SqsService {
    pub fn new(state: SharedSqsState) -> Self {
        Self { state }
    }
}

#[async_trait]
impl AwsService for SqsService {
    fn service_name(&self) -> &str {
        "sqs"
    }

    async fn handle(&self, req: AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        match req.action.as_str() {
            "CreateQueue" => self.create_queue(&req),
            "DeleteQueue" => self.delete_queue(&req),
            "ListQueues" => self.list_queues(&req),
            "GetQueueUrl" => self.get_queue_url(&req),
            "GetQueueAttributes" => self.get_queue_attributes(&req),
            "SetQueueAttributes" => self.set_queue_attributes(&req),
            "SendMessage" => self.send_message(&req),
            "SendMessageBatch" => self.send_message_batch(&req),
            "ReceiveMessage" => self.receive_message(&req).await,
            "DeleteMessage" => self.delete_message(&req),
            "DeleteMessageBatch" => self.delete_message_batch(&req),
            "PurgeQueue" => self.purge_queue(&req),
            "ChangeMessageVisibility" => self.change_message_visibility(&req),
            "ChangeMessageVisibilityBatch" => self.change_message_visibility_batch(&req),
            _ => Err(AwsServiceError::action_not_implemented("sqs", &req.action)),
        }
    }

    fn supported_actions(&self) -> &[&str] {
        &[
            "CreateQueue",
            "DeleteQueue",
            "ListQueues",
            "GetQueueUrl",
            "GetQueueAttributes",
            "SetQueueAttributes",
            "SendMessage",
            "SendMessageBatch",
            "ReceiveMessage",
            "DeleteMessage",
            "DeleteMessageBatch",
            "PurgeQueue",
            "ChangeMessageVisibility",
            "ChangeMessageVisibilityBatch",
        ]
    }
}

fn parse_body(req: &AwsRequest) -> Value {
    serde_json::from_slice(&req.body).unwrap_or(Value::Object(Default::default()))
}

fn json_response(body: Value) -> AwsResponse {
    AwsResponse::json(StatusCode::OK, serde_json::to_string(&body).unwrap())
}

impl SqsService {
    fn create_queue(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let queue_name = body["QueueName"]
            .as_str()
            .ok_or_else(|| missing_param("QueueName"))?
            .to_string();

        let mut state = self.state.write();

        if let Some(url) = state.name_to_url.get(&queue_name) {
            return Ok(json_response(json!({ "QueueUrl": url })));
        }

        let is_fifo = queue_name.ends_with(".fifo");
        let queue_url = format!("{}/{}/{}", state.endpoint, state.account_id, queue_name);

        let mut attributes = HashMap::new();
        attributes.insert("VisibilityTimeout".to_string(), "30".to_string());
        if is_fifo {
            attributes.insert("FifoQueue".to_string(), "true".to_string());
        }

        if let Some(attrs) = body["Attributes"].as_object() {
            for (k, v) in attrs {
                if let Some(s) = v.as_str() {
                    attributes.insert(k.clone(), s.to_string());
                }
            }
        }

        let redrive_policy = attributes.get("RedrivePolicy").and_then(|rp_str| {
            let rp: Value = serde_json::from_str(rp_str).ok()?;
            let dead_letter_target_arn = rp["deadLetterTargetArn"].as_str()?.to_string();
            let max_receive_count = rp["maxReceiveCount"]
                .as_u64()
                .or_else(|| rp["maxReceiveCount"].as_str()?.parse().ok())?
                as u32;
            Some(RedrivePolicy {
                dead_letter_target_arn,
                max_receive_count,
            })
        });

        let queue = SqsQueue {
            arn: format!(
                "arn:aws:sqs:{}:{}:{}",
                state.region, state.account_id, queue_name
            ),
            queue_name: queue_name.clone(),
            queue_url: queue_url.clone(),
            created_at: Utc::now(),
            messages: VecDeque::new(),
            inflight: Vec::new(),
            attributes,
            is_fifo,
            dedup_cache: HashMap::new(),
            redrive_policy,
        };

        state.name_to_url.insert(queue_name, queue_url.clone());
        state.queues.insert(queue_url.clone(), queue);

        Ok(json_response(json!({ "QueueUrl": queue_url })))
    }

    fn delete_queue(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let queue_url = body["QueueUrl"]
            .as_str()
            .ok_or_else(|| missing_param("QueueUrl"))?
            .to_string();

        let mut state = self.state.write();
        let queue = state
            .queues
            .remove(&queue_url)
            .ok_or_else(queue_not_found)?;
        state.name_to_url.remove(&queue.queue_name);

        Ok(json_response(json!({})))
    }

    fn list_queues(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let prefix = body["QueueNamePrefix"].as_str();
        let state = self.state.read();

        let urls: Vec<String> = state
            .queues
            .values()
            .filter(|q| prefix.map(|p| q.queue_name.starts_with(p)).unwrap_or(true))
            .map(|q| q.queue_url.clone())
            .collect();

        Ok(json_response(json!({ "QueueUrls": urls })))
    }

    fn get_queue_url(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let queue_name = body["QueueName"]
            .as_str()
            .ok_or_else(|| missing_param("QueueName"))?;

        let state = self.state.read();
        let url = state
            .name_to_url
            .get(queue_name)
            .ok_or_else(queue_not_found)?;

        Ok(json_response(json!({ "QueueUrl": url })))
    }

    fn get_queue_attributes(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let queue_url = body["QueueUrl"]
            .as_str()
            .ok_or_else(|| missing_param("QueueUrl"))?;

        let state = self.state.read();
        let queue = state.queues.get(queue_url).ok_or_else(queue_not_found)?;

        let mut attrs = queue.attributes.clone();
        attrs.insert("QueueArn".to_string(), queue.arn.clone());
        attrs.insert(
            "ApproximateNumberOfMessages".to_string(),
            queue.messages.len().to_string(),
        );
        attrs.insert(
            "ApproximateNumberOfMessagesNotVisible".to_string(),
            queue.inflight.len().to_string(),
        );

        // Filter by requested AttributeNames if present
        if let Some(requested) = body["AttributeNames"].as_array() {
            let names: Vec<&str> = requested.iter().filter_map(|v| v.as_str()).collect();
            if !names.is_empty() && !names.contains(&"All") {
                attrs.retain(|k, _| names.contains(&k.as_str()));
            }
        }

        Ok(json_response(json!({ "Attributes": attrs })))
    }

    fn send_message(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let queue_url = body["QueueUrl"]
            .as_str()
            .ok_or_else(|| missing_param("QueueUrl"))?
            .to_string();
        let message_body = body["MessageBody"]
            .as_str()
            .ok_or_else(|| missing_param("MessageBody"))?
            .to_string();

        let message_group_id = body["MessageGroupId"].as_str().map(|s| s.to_string());
        let message_dedup_id = body["MessageDeduplicationId"]
            .as_str()
            .map(|s| s.to_string());

        let mut state = self.state.write();
        let queue = state
            .queues
            .get_mut(&queue_url)
            .ok_or_else(queue_not_found)?;

        // FIFO validations
        if queue.is_fifo {
            if message_group_id.is_none() {
                return Err(AwsServiceError::aws_error(
                    StatusCode::BAD_REQUEST,
                    "MissingParameter",
                    "The request must contain the parameter MessageGroupId.",
                ));
            }
            if message_dedup_id.is_none()
                && queue
                    .attributes
                    .get("ContentBasedDeduplication")
                    .map(|v| v.as_str())
                    != Some("true")
            {
                return Err(AwsServiceError::aws_error(
                    StatusCode::BAD_REQUEST,
                    "MissingParameter",
                    "The request must contain the parameter MessageDeduplicationId.",
                ));
            }
        }

        // FIFO dedup
        if queue.is_fifo {
            if let Some(ref dedup_id) = message_dedup_id {
                let now = Utc::now();
                queue.dedup_cache.retain(|_, expiry| *expiry > now);
                if queue.dedup_cache.contains_key(dedup_id) {
                    let msg_id = uuid::Uuid::new_v4().to_string();
                    return Ok(json_response(json!({
                        "MessageId": msg_id,
                        "MD5OfMessageBody": md5_hex(&message_body),
                    })));
                }
                queue
                    .dedup_cache
                    .insert(dedup_id.clone(), now + chrono::Duration::minutes(5));
            }
        }

        // MaximumMessageSize validation
        let max_message_size: usize = queue
            .attributes
            .get("MaximumMessageSize")
            .and_then(|s| s.parse().ok())
            .unwrap_or(262144);
        if message_body.len() > max_message_size {
            return Err(AwsServiceError::aws_error(
                StatusCode::BAD_REQUEST,
                "InvalidParameterValue",
                format!(
                    "One or more parameters are invalid. Reason: Message must be shorter than {} bytes.",
                    max_message_size
                ),
            ));
        }

        let delay: i64 = body["DelaySeconds"]
            .as_i64()
            .or_else(|| {
                queue
                    .attributes
                    .get("DelaySeconds")
                    .and_then(|s| s.parse().ok())
            })
            .unwrap_or(0);
        let now = Utc::now();
        let visible_at = if delay > 0 {
            Some(now + chrono::Duration::seconds(delay))
        } else {
            None
        };

        let message_attributes = parse_message_attributes(&body);

        let msg = SqsMessage {
            message_id: uuid::Uuid::new_v4().to_string(),
            receipt_handle: None,
            md5_of_body: md5_hex(&message_body),
            body: message_body,
            sent_timestamp: now.timestamp_millis(),
            attributes: HashMap::new(),
            message_attributes,
            visible_at,
            receive_count: 0,
            message_group_id,
            message_dedup_id,
            created_at: now,
        };

        let resp = json!({
            "MessageId": msg.message_id,
            "MD5OfMessageBody": msg.md5_of_body,
        });
        queue.messages.push_back(msg);

        Ok(json_response(resp))
    }

    async fn receive_message(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let queue_url = body["QueueUrl"]
            .as_str()
            .ok_or_else(|| missing_param("QueueUrl"))?
            .to_string();
        let max_messages = body["MaxNumberOfMessages"].as_i64().unwrap_or(1).min(10) as usize;
        let visibility_timeout = body["VisibilityTimeout"].as_i64();
        let wait_time_seconds = body["WaitTimeSeconds"].as_i64().unwrap_or(0).clamp(0, 20) as u64;

        let deadline = if wait_time_seconds > 0 {
            Some(tokio::time::Instant::now() + std::time::Duration::from_secs(wait_time_seconds))
        } else {
            None
        };

        loop {
            let result = self.try_receive_messages(&queue_url, max_messages, visibility_timeout)?;

            if !result.is_empty() || deadline.is_none() {
                return Ok(format_receive_response(&result));
            }

            let deadline = deadline.unwrap();
            if tokio::time::Instant::now() >= deadline {
                return Ok(format_receive_response(&result));
            }

            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    fn try_receive_messages(
        &self,
        queue_url: &str,
        max_messages: usize,
        req_visibility_timeout: Option<i64>,
    ) -> Result<Vec<SqsMessage>, AwsServiceError> {
        let mut state = self.state.write();
        let queue = state
            .queues
            .get_mut(queue_url)
            .ok_or_else(queue_not_found)?;

        let visibility_timeout: i64 = req_visibility_timeout
            .or_else(|| {
                queue
                    .attributes
                    .get("VisibilityTimeout")
                    .and_then(|s| s.parse().ok())
            })
            .unwrap_or(30);

        let is_fifo = queue.is_fifo;
        let now = Utc::now();

        // MessageRetentionPeriod expiry: remove messages older than the retention period
        let retention_seconds: i64 = queue
            .attributes
            .get("MessageRetentionPeriod")
            .and_then(|s| s.parse().ok())
            .unwrap_or(345600); // default 4 days
        queue
            .messages
            .retain(|m| (now - m.created_at).num_seconds() < retention_seconds);
        queue
            .inflight
            .retain(|m| (now - m.created_at).num_seconds() < retention_seconds);

        // Return expired inflight messages
        let mut returned = Vec::new();
        queue.inflight.retain(|m| {
            if let Some(visible_at) = m.visible_at {
                if visible_at <= now {
                    returned.push(m.clone());
                    return false;
                }
            }
            true
        });
        for mut m in returned {
            m.visible_at = None;
            m.receipt_handle = None;
            queue.messages.push_back(m);
        }

        let redrive_policy = queue.redrive_policy.clone();

        let mut received = Vec::new();
        let mut remaining = VecDeque::new();
        let mut dlq_messages = Vec::new();

        // For FIFO queues, track the chosen message group
        let mut fifo_group: Option<String> = None;

        while let Some(mut msg) = queue.messages.pop_front() {
            if let Some(visible_at) = msg.visible_at {
                if visible_at > now {
                    remaining.push_back(msg);
                    continue;
                }
            }

            // FIFO: only deliver from one message group per request
            if is_fifo {
                if let Some(ref group) = msg.message_group_id {
                    match fifo_group {
                        None => fifo_group = Some(group.clone()),
                        Some(ref chosen) if chosen != group => {
                            remaining.push_back(msg);
                            continue;
                        }
                        _ => {}
                    }
                }
            }

            if received.len() < max_messages {
                msg.receive_count += 1;

                // Check DLQ redrive
                if let Some(ref rp) = redrive_policy {
                    if msg.receive_count >= rp.max_receive_count {
                        dlq_messages.push((rp.dead_letter_target_arn.clone(), msg));
                        continue;
                    }
                }

                msg.receipt_handle = Some(uuid::Uuid::new_v4().to_string());
                msg.visible_at = Some(now + chrono::Duration::seconds(visibility_timeout));
                received.push(msg);
            } else {
                remaining.push_back(msg);
                break;
            }
        }

        while let Some(m) = queue.messages.pop_front() {
            remaining.push_back(m);
        }
        queue.messages = remaining;

        for msg in &received {
            queue.inflight.push(msg.clone());
        }

        // Move messages to DLQ
        for (dlq_arn, mut msg) in dlq_messages {
            if let Some(dlq) = state.queues.values_mut().find(|q| q.arn == dlq_arn) {
                msg.receipt_handle = None;
                msg.visible_at = None;
                dlq.messages.push_back(msg);
            }
        }

        Ok(received)
    }

    fn delete_message(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let queue_url = body["QueueUrl"]
            .as_str()
            .ok_or_else(|| missing_param("QueueUrl"))?;
        let receipt_handle = body["ReceiptHandle"]
            .as_str()
            .ok_or_else(|| missing_param("ReceiptHandle"))?;

        let mut state = self.state.write();
        let queue = state
            .queues
            .get_mut(queue_url)
            .ok_or_else(queue_not_found)?;

        queue
            .inflight
            .retain(|m| m.receipt_handle.as_deref() != Some(receipt_handle));

        Ok(json_response(json!({})))
    }

    fn purge_queue(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let queue_url = body["QueueUrl"]
            .as_str()
            .ok_or_else(|| missing_param("QueueUrl"))?;

        let mut state = self.state.write();
        let queue = state
            .queues
            .get_mut(queue_url)
            .ok_or_else(queue_not_found)?;

        queue.messages.clear();
        queue.inflight.clear();

        Ok(json_response(json!({})))
    }

    fn change_message_visibility(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let queue_url = body["QueueUrl"]
            .as_str()
            .ok_or_else(|| missing_param("QueueUrl"))?;
        let receipt_handle = body["ReceiptHandle"]
            .as_str()
            .ok_or_else(|| missing_param("ReceiptHandle"))?;
        let visibility_timeout = body["VisibilityTimeout"]
            .as_i64()
            .ok_or_else(|| missing_param("VisibilityTimeout"))?;

        let mut state = self.state.write();
        let queue = state
            .queues
            .get_mut(queue_url)
            .ok_or_else(queue_not_found)?;

        let now = Utc::now();
        for msg in &mut queue.inflight {
            if msg.receipt_handle.as_deref() == Some(receipt_handle) {
                msg.visible_at = Some(now + chrono::Duration::seconds(visibility_timeout));
                break;
            }
        }

        Ok(json_response(json!({})))
    }

    fn change_message_visibility_batch(
        &self,
        req: &AwsRequest,
    ) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let queue_url = body["QueueUrl"]
            .as_str()
            .ok_or_else(|| missing_param("QueueUrl"))?;

        let entries = body["Entries"]
            .as_array()
            .ok_or_else(|| missing_param("Entries"))?
            .clone();

        let mut state = self.state.write();
        let queue = state
            .queues
            .get_mut(queue_url)
            .ok_or_else(queue_not_found)?;

        let now = Utc::now();
        let mut successful = Vec::new();
        let mut failed: Vec<Value> = Vec::new();

        for entry in &entries {
            let id = match entry["Id"].as_str() {
                Some(id) => id.to_string(),
                None => continue,
            };
            let receipt_handle = match entry["ReceiptHandle"].as_str() {
                Some(rh) => rh,
                None => {
                    failed.push(json!({
                        "Id": id,
                        "SenderFault": true,
                        "Code": "MissingParameter",
                        "Message": "ReceiptHandle is required",
                    }));
                    continue;
                }
            };
            let visibility_timeout = match entry["VisibilityTimeout"].as_i64() {
                Some(vt) => vt,
                None => {
                    failed.push(json!({
                        "Id": id,
                        "SenderFault": true,
                        "Code": "MissingParameter",
                        "Message": "VisibilityTimeout is required",
                    }));
                    continue;
                }
            };

            let mut found = false;
            for msg in &mut queue.inflight {
                if msg.receipt_handle.as_deref() == Some(receipt_handle) {
                    msg.visible_at = Some(now + chrono::Duration::seconds(visibility_timeout));
                    found = true;
                    break;
                }
            }

            if found {
                successful.push(json!({ "Id": id }));
            } else {
                failed.push(json!({
                    "Id": id,
                    "SenderFault": true,
                    "Code": "ReceiptHandleIsInvalid",
                    "Message": "The input receipt handle is invalid.",
                }));
            }
        }

        Ok(json_response(json!({
            "Successful": successful,
            "Failed": failed,
        })))
    }

    fn set_queue_attributes(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let queue_url = body["QueueUrl"]
            .as_str()
            .ok_or_else(|| missing_param("QueueUrl"))?;

        let mut state = self.state.write();
        let queue = state
            .queues
            .get_mut(queue_url)
            .ok_or_else(queue_not_found)?;

        if let Some(attrs) = body["Attributes"].as_object() {
            for (k, v) in attrs {
                if let Some(s) = v.as_str() {
                    queue.attributes.insert(k.clone(), s.to_string());
                }
            }

            // Update redrive_policy if set
            if let Some(rp_str) = attrs.get("RedrivePolicy").and_then(|v| v.as_str()) {
                if let Ok(rp) = serde_json::from_str::<Value>(rp_str) {
                    let dead_letter_target_arn = rp["deadLetterTargetArn"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string();
                    let max_receive_count = rp["maxReceiveCount"]
                        .as_u64()
                        .or_else(|| rp["maxReceiveCount"].as_str()?.parse().ok())
                        .unwrap_or(0) as u32;
                    if !dead_letter_target_arn.is_empty() && max_receive_count > 0 {
                        queue.redrive_policy = Some(RedrivePolicy {
                            dead_letter_target_arn,
                            max_receive_count,
                        });
                    }
                }
            }
        }

        Ok(json_response(json!({})))
    }

    fn send_message_batch(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let queue_url = body["QueueUrl"]
            .as_str()
            .ok_or_else(|| missing_param("QueueUrl"))?
            .to_string();

        let entries = body["Entries"]
            .as_array()
            .ok_or_else(|| missing_param("Entries"))?
            .clone();

        let mut state = self.state.write();
        let queue = state
            .queues
            .get_mut(&queue_url)
            .ok_or_else(queue_not_found)?;

        let now = Utc::now();
        let mut successful = Vec::new();
        let mut failed: Vec<Value> = Vec::new();

        let is_fifo = queue.is_fifo;
        let content_based_dedup = queue
            .attributes
            .get("ContentBasedDeduplication")
            .map(|v| v.as_str())
            == Some("true");
        let max_message_size: usize = queue
            .attributes
            .get("MaximumMessageSize")
            .and_then(|s| s.parse().ok())
            .unwrap_or(262144);
        let queue_delay: Option<i64> = queue
            .attributes
            .get("DelaySeconds")
            .and_then(|s| s.parse().ok());

        for entry in &entries {
            let id = match entry["Id"].as_str() {
                Some(id) => id.to_string(),
                None => continue,
            };
            let message_body = match entry["MessageBody"].as_str() {
                Some(b) => b.to_string(),
                None => {
                    failed.push(json!({
                        "Id": id,
                        "SenderFault": true,
                        "Code": "MissingParameter",
                        "Message": "MessageBody is required",
                    }));
                    continue;
                }
            };

            // MaximumMessageSize validation
            if message_body.len() > max_message_size {
                failed.push(json!({
                    "Id": id,
                    "SenderFault": true,
                    "Code": "InvalidParameterValue",
                    "Message": format!(
                        "One or more parameters are invalid. Reason: Message must be shorter than {} bytes.",
                        max_message_size
                    ),
                }));
                continue;
            }

            let message_group_id = entry["MessageGroupId"].as_str().map(|s| s.to_string());
            let message_dedup_id = entry["MessageDeduplicationId"]
                .as_str()
                .map(|s| s.to_string());

            // FIFO validations
            if is_fifo {
                if message_group_id.is_none() {
                    failed.push(json!({
                        "Id": id,
                        "SenderFault": true,
                        "Code": "MissingParameter",
                        "Message": "The request must contain the parameter MessageGroupId.",
                    }));
                    continue;
                }
                if message_dedup_id.is_none() && !content_based_dedup {
                    failed.push(json!({
                        "Id": id,
                        "SenderFault": true,
                        "Code": "MissingParameter",
                        "Message": "The request must contain the parameter MessageDeduplicationId.",
                    }));
                    continue;
                }
            }

            let delay: i64 = entry["DelaySeconds"].as_i64().or(queue_delay).unwrap_or(0);
            let visible_at = if delay > 0 {
                Some(now + chrono::Duration::seconds(delay))
            } else {
                None
            };

            let message_attributes = parse_message_attributes(entry);

            let msg = SqsMessage {
                message_id: uuid::Uuid::new_v4().to_string(),
                receipt_handle: None,
                md5_of_body: md5_hex(&message_body),
                body: message_body,
                sent_timestamp: now.timestamp_millis(),
                attributes: HashMap::new(),
                message_attributes,
                visible_at,
                receive_count: 0,
                message_group_id,
                message_dedup_id,
                created_at: now,
            };

            successful.push(json!({
                "Id": id,
                "MessageId": msg.message_id,
                "MD5OfMessageBody": msg.md5_of_body,
            }));
            queue.messages.push_back(msg);
        }

        Ok(json_response(json!({
            "Successful": successful,
            "Failed": failed,
        })))
    }

    fn delete_message_batch(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let queue_url = body["QueueUrl"]
            .as_str()
            .ok_or_else(|| missing_param("QueueUrl"))?;

        let entries = body["Entries"]
            .as_array()
            .ok_or_else(|| missing_param("Entries"))?
            .clone();

        let mut state = self.state.write();
        let queue = state
            .queues
            .get_mut(queue_url)
            .ok_or_else(queue_not_found)?;

        let mut successful = Vec::new();
        let mut failed: Vec<Value> = Vec::new();

        for entry in &entries {
            let id = match entry["Id"].as_str() {
                Some(id) => id.to_string(),
                None => continue,
            };
            let receipt_handle = match entry["ReceiptHandle"].as_str() {
                Some(rh) => rh,
                None => {
                    failed.push(json!({
                        "Id": id,
                        "SenderFault": true,
                        "Code": "MissingParameter",
                        "Message": "ReceiptHandle is required",
                    }));
                    continue;
                }
            };

            queue
                .inflight
                .retain(|m| m.receipt_handle.as_deref() != Some(receipt_handle));

            successful.push(json!({ "Id": id }));
        }

        Ok(json_response(json!({
            "Successful": successful,
            "Failed": failed,
        })))
    }
}

fn format_receive_response(received: &[SqsMessage]) -> AwsResponse {
    let messages: Vec<Value> = received
        .iter()
        .map(|m| {
            let mut msg_json = json!({
                "MessageId": m.message_id,
                "ReceiptHandle": m.receipt_handle,
                "MD5OfBody": m.md5_of_body,
                "Body": m.body,
            });
            if !m.message_attributes.is_empty() {
                let attrs: serde_json::Map<String, Value> = m
                    .message_attributes
                    .iter()
                    .map(|(k, v)| {
                        let mut attr = json!({ "DataType": v.data_type });
                        if let Some(ref sv) = v.string_value {
                            attr["StringValue"] = json!(sv);
                        }
                        (k.clone(), attr)
                    })
                    .collect();
                msg_json["MessageAttributes"] = Value::Object(attrs);
            }
            msg_json
        })
        .collect();

    json_response(json!({ "Messages": messages }))
}

fn parse_message_attributes(body: &Value) -> HashMap<String, MessageAttribute> {
    let mut result = HashMap::new();
    if let Some(attrs) = body["MessageAttributes"].as_object() {
        for (name, val) in attrs {
            let data_type = val["DataType"].as_str().unwrap_or("String").to_string();
            let string_value = val["StringValue"].as_str().map(|s| s.to_string());
            result.insert(
                name.clone(),
                MessageAttribute {
                    data_type,
                    string_value,
                },
            );
        }
    }
    result
}

fn missing_param(name: &str) -> AwsServiceError {
    AwsServiceError::aws_error(
        StatusCode::BAD_REQUEST,
        "MissingParameter",
        format!("The request must contain the parameter {name}"),
    )
}

fn queue_not_found() -> AwsServiceError {
    AwsServiceError::aws_error(
        StatusCode::BAD_REQUEST,
        "AWS.SimpleQueueService.NonExistentQueue",
        "The specified queue does not exist.",
    )
}

pub fn md5_hex(input: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(input.as_bytes());
    format!("{:032x}", hasher.finalize())
}
