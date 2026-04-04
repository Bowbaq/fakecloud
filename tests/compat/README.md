# FakeCloud Compatibility Tests

Comprehensive boto3 compatibility test suite that exercises all supported AWS service actions against FakeCloud.

## Prerequisites

- Python 3.8+ with `boto3` installed
- Rust toolchain (for building FakeCloud)

## Running

```bash
# Automated: build, start server, run tests, stop server
./tests/compat/run.sh

# Manual: if server is already running on localhost:4566
python3 tests/compat/boto3_compat.py
```

## Services Tested

| Service | Actions Tested |
|---------|---------------|
| SQS | CreateQueue, ListQueues, GetQueueUrl, GetQueueAttributes, SetQueueAttributes, SendMessage, SendMessageBatch, ReceiveMessage, DeleteMessage, DeleteMessageBatch, ChangeMessageVisibility, ChangeMessageVisibilityBatch, PurgeQueue, DeleteQueue, FIFO ordering, FIFO dedup, DLQ |
| SNS | CreateTopic, ListTopics, GetTopicAttributes, SetTopicAttributes, Subscribe, ListSubscriptions, ListSubscriptionsByTopic, GetSubscriptionAttributes, Publish, TagResource, ListTagsForResource, UntagResource, Unsubscribe, DeleteTopic, SNS->SQS delivery |
| EventBridge | CreateEventBus, ListEventBuses, DescribeEventBus, PutRule, ListRules, DescribeRule, PutTargets, ListTargetsByRule, PutEvents, RemoveTargets, TagResource, ListTagsForResource, UntagResource, DeleteRule, DeleteEventBus, EB->SQS delivery |
| IAM | CreateUser, GetUser, ListUsers, CreateAccessKey, ListAccessKeys, DeleteAccessKey, CreateRole, GetRole, ListRoles, CreatePolicy, GetPolicy, ListPolicies, GetPolicyVersion, AttachRolePolicy, ListAttachedRolePolicies, DetachRolePolicy, DeletePolicy, DeleteRole, DeleteUser |
| STS | GetCallerIdentity, AssumeRole |
| SSM | PutParameter, GetParameter, GetParameters, GetParametersByPath, DescribeParameters, GetParameterHistory, AddTagsToResource, ListTagsForResource, RemoveTagsFromResource, DeleteParameter, DeleteParameters |
| Cross-Service | SNS->SQS fan-out, EventBridge->SQS, EventBridge->SNS->SQS chain |
