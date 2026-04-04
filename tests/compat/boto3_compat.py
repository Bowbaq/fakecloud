#!/usr/bin/env python3
"""
FakeCloud Compatibility Test Suite

Tests all supported AWS service actions against FakeCloud using boto3.
Reports pass/fail for each action with a summary table.
"""

import json
import sys
import time
import traceback
import uuid

import boto3
from botocore.config import Config
from botocore.exceptions import ClientError

ENDPOINT = "http://localhost:4566"
REGION = "us-east-1"

# Counters
results = {}  # {service: [(test_name, passed, error_msg)]}


def client(service_name, **kwargs):
    return boto3.client(
        service_name,
        endpoint_url=ENDPOINT,
        region_name=REGION,
        aws_access_key_id="testing",
        aws_secret_access_key="testing",
        config=Config(retries={"max_attempts": 0}),
        **kwargs,
    )


def record(service, test_name, passed, error_msg=""):
    results.setdefault(service, [])
    results[service].append((test_name, passed, error_msg))
    status = "PASS" if passed else "FAIL"
    print(f"  [{status}] {test_name}")
    if not passed and error_msg:
        for line in error_msg.strip().split("\n"):
            print(f"         {line}")


def run_test(service, test_name, fn):
    try:
        fn()
        record(service, test_name, True)
    except Exception as e:
        msg = str(e)
        if len(msg) > 300:
            msg = msg[:300] + "..."
        record(service, test_name, False, msg)


def uid():
    return uuid.uuid4().hex[:8]


# ---------------------------------------------------------------------------
# SQS Tests
# ---------------------------------------------------------------------------
def test_sqs():
    sqs = client("sqs")
    print("\n=== SQS Tests ===")

    queue_name = f"test-queue-{uid()}"
    queue_url = None
    fifo_queue_url = None

    # CreateQueue (standard)
    def t_create_queue():
        nonlocal queue_url
        resp = sqs.create_queue(QueueName=queue_name)
        queue_url = resp["QueueUrl"]
        assert queue_url, "No QueueUrl returned"
    run_test("SQS", "CreateQueue (standard)", t_create_queue)

    # CreateQueue (FIFO)
    def t_create_fifo():
        nonlocal fifo_queue_url
        name = f"test-fifo-{uid()}.fifo"
        resp = sqs.create_queue(
            QueueName=name,
            Attributes={"FifoQueue": "true", "ContentBasedDeduplication": "true"},
        )
        fifo_queue_url = resp["QueueUrl"]
        assert fifo_queue_url, "No QueueUrl returned"
    run_test("SQS", "CreateQueue (FIFO)", t_create_fifo)

    # ListQueues
    def t_list_queues():
        resp = sqs.list_queues()
        urls = resp.get("QueueUrls", [])
        assert any(queue_name in u for u in urls), f"Queue not in list: {urls}"
    run_test("SQS", "ListQueues", t_list_queues)

    # ListQueues with prefix
    def t_list_queues_prefix():
        resp = sqs.list_queues(QueueNamePrefix=queue_name[:10])
        urls = resp.get("QueueUrls", [])
        assert any(queue_name in u for u in urls), f"Queue not in prefix list"
    run_test("SQS", "ListQueues (prefix filter)", t_list_queues_prefix)

    # GetQueueUrl
    def t_get_queue_url():
        resp = sqs.get_queue_url(QueueName=queue_name)
        assert resp["QueueUrl"] == queue_url, f"URL mismatch: {resp['QueueUrl']} != {queue_url}"
    run_test("SQS", "GetQueueUrl", t_get_queue_url)

    # GetQueueAttributes (All)
    def t_get_attrs_all():
        resp = sqs.get_queue_attributes(QueueUrl=queue_url, AttributeNames=["All"])
        attrs = resp.get("Attributes", {})
        assert "QueueArn" in attrs, f"No QueueArn in attributes: {list(attrs.keys())}"
    run_test("SQS", "GetQueueAttributes (All)", t_get_attrs_all)

    # GetQueueAttributes (specific)
    def t_get_attrs_specific():
        resp = sqs.get_queue_attributes(
            QueueUrl=queue_url,
            AttributeNames=["ApproximateNumberOfMessages", "VisibilityTimeout"],
        )
        attrs = resp.get("Attributes", {})
        assert "VisibilityTimeout" in attrs, f"Missing VisibilityTimeout: {list(attrs.keys())}"
    run_test("SQS", "GetQueueAttributes (specific)", t_get_attrs_specific)

    # SetQueueAttributes
    def t_set_attrs():
        sqs.set_queue_attributes(
            QueueUrl=queue_url, Attributes={"VisibilityTimeout": "60"}
        )
        resp = sqs.get_queue_attributes(
            QueueUrl=queue_url, AttributeNames=["VisibilityTimeout"]
        )
        assert resp["Attributes"]["VisibilityTimeout"] == "60"
    run_test("SQS", "SetQueueAttributes", t_set_attrs)

    # SendMessage
    def t_send():
        resp = sqs.send_message(QueueUrl=queue_url, MessageBody="hello")
        assert "MessageId" in resp, "No MessageId"
    run_test("SQS", "SendMessage", t_send)

    # SendMessage with attributes
    def t_send_attrs():
        resp = sqs.send_message(
            QueueUrl=queue_url,
            MessageBody="with attrs",
            MessageAttributes={
                "TestAttr": {"DataType": "String", "StringValue": "val1"}
            },
        )
        assert "MessageId" in resp
    run_test("SQS", "SendMessage (with attributes)", t_send_attrs)

    # SendMessage with delay
    def t_send_delay():
        resp = sqs.send_message(
            QueueUrl=queue_url, MessageBody="delayed", DelaySeconds=0
        )
        assert "MessageId" in resp
    run_test("SQS", "SendMessage (with delay)", t_send_delay)

    # SendMessageBatch
    def t_send_batch():
        resp = sqs.send_message_batch(
            QueueUrl=queue_url,
            Entries=[
                {"Id": "1", "MessageBody": "batch1"},
                {"Id": "2", "MessageBody": "batch2"},
            ],
        )
        assert len(resp.get("Successful", [])) == 2, f"Expected 2 successful: {resp}"
    run_test("SQS", "SendMessageBatch", t_send_batch)

    # ReceiveMessage
    def t_receive():
        resp = sqs.receive_message(
            QueueUrl=queue_url, MaxNumberOfMessages=10, WaitTimeSeconds=0
        )
        msgs = resp.get("Messages", [])
        assert len(msgs) > 0, "No messages received"
    run_test("SQS", "ReceiveMessage", t_receive)

    # ReceiveMessage with attributes
    def t_receive_attrs():
        # Send a message with attributes first
        sqs.send_message(
            QueueUrl=queue_url,
            MessageBody="attr-test",
            MessageAttributes={
                "Foo": {"DataType": "String", "StringValue": "bar"}
            },
        )
        time.sleep(0.2)
        resp = sqs.receive_message(
            QueueUrl=queue_url,
            MaxNumberOfMessages=10,
            MessageAttributeNames=["All"],
            WaitTimeSeconds=0,
        )
        msgs = resp.get("Messages", [])
        assert len(msgs) > 0, "No messages received"
    run_test("SQS", "ReceiveMessage (with attribute names)", t_receive_attrs)

    # DeleteMessage
    def t_delete():
        resp = sqs.receive_message(QueueUrl=queue_url, MaxNumberOfMessages=1, WaitTimeSeconds=0)
        msgs = resp.get("Messages", [])
        if msgs:
            sqs.delete_message(
                QueueUrl=queue_url, ReceiptHandle=msgs[0]["ReceiptHandle"]
            )
        else:
            raise AssertionError("No message to delete")
    run_test("SQS", "DeleteMessage", t_delete)

    # DeleteMessageBatch
    def t_delete_batch():
        sqs.send_message(QueueUrl=queue_url, MessageBody="del-batch-1")
        sqs.send_message(QueueUrl=queue_url, MessageBody="del-batch-2")
        time.sleep(0.2)
        resp = sqs.receive_message(QueueUrl=queue_url, MaxNumberOfMessages=5, WaitTimeSeconds=0)
        msgs = resp.get("Messages", [])
        if len(msgs) >= 2:
            sqs.delete_message_batch(
                QueueUrl=queue_url,
                Entries=[
                    {"Id": str(i), "ReceiptHandle": m["ReceiptHandle"]}
                    for i, m in enumerate(msgs[:2])
                ],
            )
        elif len(msgs) == 1:
            sqs.delete_message_batch(
                QueueUrl=queue_url,
                Entries=[{"Id": "0", "ReceiptHandle": msgs[0]["ReceiptHandle"]}],
            )
        else:
            raise AssertionError("No messages for batch delete")
    run_test("SQS", "DeleteMessageBatch", t_delete_batch)

    # ChangeMessageVisibility
    def t_change_vis():
        sqs.send_message(QueueUrl=queue_url, MessageBody="vis-test")
        time.sleep(0.2)
        resp = sqs.receive_message(QueueUrl=queue_url, MaxNumberOfMessages=1, WaitTimeSeconds=0)
        msgs = resp.get("Messages", [])
        assert msgs, "No message to change visibility"
        sqs.change_message_visibility(
            QueueUrl=queue_url,
            ReceiptHandle=msgs[0]["ReceiptHandle"],
            VisibilityTimeout=0,
        )
    run_test("SQS", "ChangeMessageVisibility", t_change_vis)

    # ChangeMessageVisibilityBatch
    def t_change_vis_batch():
        sqs.send_message(QueueUrl=queue_url, MessageBody="vis-batch-1")
        time.sleep(0.2)
        resp = sqs.receive_message(QueueUrl=queue_url, MaxNumberOfMessages=5, WaitTimeSeconds=0)
        msgs = resp.get("Messages", [])
        assert msgs, "No messages for batch visibility change"
        sqs.change_message_visibility_batch(
            QueueUrl=queue_url,
            Entries=[
                {
                    "Id": str(i),
                    "ReceiptHandle": m["ReceiptHandle"],
                    "VisibilityTimeout": 0,
                }
                for i, m in enumerate(msgs)
            ],
        )
    run_test("SQS", "ChangeMessageVisibilityBatch", t_change_vis_batch)

    # PurgeQueue
    def t_purge():
        sqs.send_message(QueueUrl=queue_url, MessageBody="purge-me")
        sqs.purge_queue(QueueUrl=queue_url)
        time.sleep(0.3)
        resp = sqs.receive_message(QueueUrl=queue_url, MaxNumberOfMessages=1, WaitTimeSeconds=0)
        msgs = resp.get("Messages", [])
        assert len(msgs) == 0, f"Queue not purged, got {len(msgs)} messages"
    run_test("SQS", "PurgeQueue", t_purge)

    # FIFO: MessageGroupId ordering
    def t_fifo_ordering():
        if not fifo_queue_url:
            raise AssertionError("FIFO queue not created")
        for i in range(3):
            sqs.send_message(
                QueueUrl=fifo_queue_url,
                MessageBody=f"fifo-{i}",
                MessageGroupId="group1",
            )
        time.sleep(0.2)
        resp = sqs.receive_message(
            QueueUrl=fifo_queue_url, MaxNumberOfMessages=3, WaitTimeSeconds=0
        )
        msgs = resp.get("Messages", [])
        assert len(msgs) >= 1, "No FIFO messages received"
        # At least first message should be fifo-0
        assert msgs[0]["Body"] == "fifo-0", f"Expected fifo-0, got {msgs[0]['Body']}"
    run_test("SQS", "FIFO: MessageGroupId ordering", t_fifo_ordering)

    # FIFO: deduplication
    def t_fifo_dedup():
        if not fifo_queue_url:
            raise AssertionError("FIFO queue not created")
        # Purge first
        sqs.purge_queue(QueueUrl=fifo_queue_url)
        time.sleep(0.3)
        dedup_id = uid()
        sqs.send_message(
            QueueUrl=fifo_queue_url,
            MessageBody="dedup-test",
            MessageGroupId="group2",
            MessageDeduplicationId=dedup_id,
        )
        sqs.send_message(
            QueueUrl=fifo_queue_url,
            MessageBody="dedup-test",
            MessageGroupId="group2",
            MessageDeduplicationId=dedup_id,
        )
        time.sleep(0.2)
        resp = sqs.receive_message(
            QueueUrl=fifo_queue_url, MaxNumberOfMessages=10, WaitTimeSeconds=0
        )
        msgs = resp.get("Messages", [])
        assert len(msgs) == 1, f"Expected 1 message after dedup, got {len(msgs)}"
    run_test("SQS", "FIFO: deduplication", t_fifo_dedup)

    # DLQ: redrive policy
    def t_dlq():
        dlq_name = f"test-dlq-{uid()}"
        dlq_resp = sqs.create_queue(QueueName=dlq_name)
        dlq_url = dlq_resp["QueueUrl"]
        dlq_attrs = sqs.get_queue_attributes(
            QueueUrl=dlq_url, AttributeNames=["QueueArn"]
        )
        dlq_arn = dlq_attrs["Attributes"]["QueueArn"]

        src_name = f"test-src-{uid()}"
        src_resp = sqs.create_queue(
            QueueName=src_name,
            Attributes={
                "RedrivePolicy": json.dumps(
                    {"deadLetterTargetArn": dlq_arn, "maxReceiveCount": "1"}
                )
            },
        )
        src_url = src_resp["QueueUrl"]
        src_attrs = sqs.get_queue_attributes(
            QueueUrl=src_url, AttributeNames=["RedrivePolicy"]
        )
        assert "RedrivePolicy" in src_attrs.get("Attributes", {}), "RedrivePolicy not set"
    run_test("SQS", "DLQ: redrive policy", t_dlq)

    # DeleteQueue
    def t_delete_queue():
        tmp_name = f"test-del-{uid()}"
        tmp = sqs.create_queue(QueueName=tmp_name)
        sqs.delete_queue(QueueUrl=tmp["QueueUrl"])
        time.sleep(0.2)
        try:
            sqs.get_queue_url(QueueName=tmp_name)
            raise AssertionError("Queue still exists after delete")
        except ClientError as e:
            code = e.response["Error"]["Code"]
            assert code in (
                "AWS.SimpleQueueService.NonExistentQueue",
                "QueueDoesNotExist",
            ), f"Unexpected error: {code}"
    run_test("SQS", "DeleteQueue", t_delete_queue)

    # Cleanup
    for url in [queue_url, fifo_queue_url]:
        if url:
            try:
                sqs.delete_queue(QueueUrl=url)
            except Exception:
                pass


# ---------------------------------------------------------------------------
# SNS Tests
# ---------------------------------------------------------------------------
def test_sns():
    sns = client("sns")
    sqs = client("sqs")
    print("\n=== SNS Tests ===")

    topic_arn = None
    topic_name = f"test-topic-{uid()}"

    # CreateTopic (standard)
    def t_create_topic():
        nonlocal topic_arn
        resp = sns.create_topic(Name=topic_name)
        topic_arn = resp["TopicArn"]
        assert topic_arn, "No TopicArn"
    run_test("SNS", "CreateTopic (standard)", t_create_topic)

    # CreateTopic (FIFO)
    fifo_topic_arn = None
    def t_create_fifo_topic():
        nonlocal fifo_topic_arn
        name = f"test-fifo-{uid()}.fifo"
        resp = sns.create_topic(
            Name=name, Attributes={"FifoTopic": "true"}
        )
        fifo_topic_arn = resp["TopicArn"]
        assert fifo_topic_arn
    run_test("SNS", "CreateTopic (FIFO)", t_create_fifo_topic)

    # ListTopics
    def t_list_topics():
        resp = sns.list_topics()
        arns = [t["TopicArn"] for t in resp.get("Topics", [])]
        assert topic_arn in arns, f"Topic not in list"
    run_test("SNS", "ListTopics", t_list_topics)

    # GetTopicAttributes
    def t_get_topic_attrs():
        resp = sns.get_topic_attributes(TopicArn=topic_arn)
        attrs = resp.get("Attributes", {})
        assert "TopicArn" in attrs, f"No TopicArn in attrs"
    run_test("SNS", "GetTopicAttributes", t_get_topic_attrs)

    # SetTopicAttributes
    def t_set_topic_attrs():
        sns.set_topic_attributes(
            TopicArn=topic_arn,
            AttributeName="DisplayName",
            AttributeValue="TestDisplay",
        )
        resp = sns.get_topic_attributes(TopicArn=topic_arn)
        assert resp["Attributes"].get("DisplayName") == "TestDisplay"
    run_test("SNS", "SetTopicAttributes", t_set_topic_attrs)

    # Subscribe (SQS)
    sub_arn = None
    sub_queue_url = None
    sub_queue_arn = None
    def t_subscribe():
        nonlocal sub_arn, sub_queue_url, sub_queue_arn
        q_name = f"sns-sub-{uid()}"
        sub_queue_url_resp = sqs.create_queue(QueueName=q_name)
        sub_queue_url = sub_queue_url_resp["QueueUrl"]
        attrs = sqs.get_queue_attributes(QueueUrl=sub_queue_url, AttributeNames=["QueueArn"])
        sub_queue_arn = attrs["Attributes"]["QueueArn"]
        resp = sns.subscribe(
            TopicArn=topic_arn, Protocol="sqs", Endpoint=sub_queue_arn
        )
        sub_arn = resp["SubscriptionArn"]
        assert sub_arn and sub_arn != "PendingConfirmation"
    run_test("SNS", "Subscribe (SQS protocol)", t_subscribe)

    # ListSubscriptions
    def t_list_subs():
        resp = sns.list_subscriptions()
        arns = [s["SubscriptionArn"] for s in resp.get("Subscriptions", [])]
        assert sub_arn in arns, f"Subscription not listed"
    run_test("SNS", "ListSubscriptions", t_list_subs)

    # ListSubscriptionsByTopic
    def t_list_subs_topic():
        resp = sns.list_subscriptions_by_topic(TopicArn=topic_arn)
        arns = [s["SubscriptionArn"] for s in resp.get("Subscriptions", [])]
        assert sub_arn in arns
    run_test("SNS", "ListSubscriptionsByTopic", t_list_subs_topic)

    # GetSubscriptionAttributes
    def t_get_sub_attrs():
        resp = sns.get_subscription_attributes(SubscriptionArn=sub_arn)
        attrs = resp.get("Attributes", {})
        assert "SubscriptionArn" in attrs
    run_test("SNS", "GetSubscriptionAttributes", t_get_sub_attrs)

    # Publish
    def t_publish():
        resp = sns.publish(TopicArn=topic_arn, Message="test-message")
        assert "MessageId" in resp
    run_test("SNS", "Publish", t_publish)

    # Publish with MessageAttributes
    def t_publish_attrs():
        resp = sns.publish(
            TopicArn=topic_arn,
            Message="with-attrs",
            MessageAttributes={
                "attr1": {"DataType": "String", "StringValue": "val1"}
            },
        )
        assert "MessageId" in resp
    run_test("SNS", "Publish (with MessageAttributes)", t_publish_attrs)

    # SNS -> SQS delivery
    def t_sns_sqs_delivery():
        if not sub_queue_url:
            raise AssertionError("No subscription queue")
        # Purge first
        sqs.purge_queue(QueueUrl=sub_queue_url)
        time.sleep(0.3)
        sns.publish(TopicArn=topic_arn, Message="fanout-test")
        time.sleep(1)
        resp = sqs.receive_message(QueueUrl=sub_queue_url, MaxNumberOfMessages=10, WaitTimeSeconds=2)
        msgs = resp.get("Messages", [])
        assert len(msgs) > 0, "No messages delivered from SNS to SQS"
        # The body should be an SNS notification JSON wrapper
        body = json.loads(msgs[0]["Body"])
        assert body.get("Message") == "fanout-test" or msgs[0]["Body"] == "fanout-test"
    run_test("SNS", "SNS -> SQS delivery", t_sns_sqs_delivery)

    # TagResource
    def t_tag():
        sns.tag_resource(ResourceArn=topic_arn, Tags=[{"Key": "env", "Value": "test"}])
    run_test("SNS", "TagResource", t_tag)

    # ListTagsForResource
    def t_list_tags():
        resp = sns.list_tags_for_resource(ResourceArn=topic_arn)
        tags = resp.get("Tags", [])
        assert any(t["Key"] == "env" and t["Value"] == "test" for t in tags)
    run_test("SNS", "ListTagsForResource", t_list_tags)

    # UntagResource
    def t_untag():
        sns.untag_resource(ResourceArn=topic_arn, TagKeys=["env"])
        resp = sns.list_tags_for_resource(ResourceArn=topic_arn)
        tags = resp.get("Tags", [])
        assert not any(t["Key"] == "env" for t in tags), "Tag not removed"
    run_test("SNS", "UntagResource", t_untag)

    # Unsubscribe
    def t_unsub():
        sns.unsubscribe(SubscriptionArn=sub_arn)
        resp = sns.list_subscriptions_by_topic(TopicArn=topic_arn)
        arns = [s["SubscriptionArn"] for s in resp.get("Subscriptions", [])]
        assert sub_arn not in arns, "Subscription not removed"
    run_test("SNS", "Unsubscribe", t_unsub)

    # DeleteTopic
    def t_delete_topic():
        tmp_arn = sns.create_topic(Name=f"del-{uid()}")["TopicArn"]
        sns.delete_topic(TopicArn=tmp_arn)
        resp = sns.list_topics()
        arns = [t["TopicArn"] for t in resp.get("Topics", [])]
        assert tmp_arn not in arns, "Topic not deleted"
    run_test("SNS", "DeleteTopic", t_delete_topic)

    # Cleanup
    try:
        sns.delete_topic(TopicArn=topic_arn)
    except Exception:
        pass
    if fifo_topic_arn:
        try:
            sns.delete_topic(TopicArn=fifo_topic_arn)
        except Exception:
            pass
    if sub_queue_url:
        try:
            sqs.delete_queue(QueueUrl=sub_queue_url)
        except Exception:
            pass


# ---------------------------------------------------------------------------
# EventBridge Tests
# ---------------------------------------------------------------------------
def test_eventbridge():
    eb = client("events")
    sqs = client("sqs")
    print("\n=== EventBridge Tests ===")

    bus_name = f"test-bus-{uid()}"
    bus_arn = None

    # CreateEventBus
    def t_create_bus():
        nonlocal bus_arn
        resp = eb.create_event_bus(Name=bus_name)
        bus_arn = resp.get("EventBusArn")
        assert bus_arn, f"No EventBusArn: {resp}"
    run_test("EventBridge", "CreateEventBus", t_create_bus)

    # ListEventBuses
    def t_list_buses():
        resp = eb.list_event_buses()
        names = [b["Name"] for b in resp.get("EventBuses", [])]
        assert bus_name in names, f"Bus not in list: {names}"
    run_test("EventBridge", "ListEventBuses", t_list_buses)

    # DescribeEventBus
    def t_describe_bus():
        resp = eb.describe_event_bus(Name=bus_name)
        assert resp.get("Name") == bus_name
    run_test("EventBridge", "DescribeEventBus", t_describe_bus)

    # PutRule (event pattern)
    rule_name = f"test-rule-{uid()}"
    def t_put_rule_pattern():
        resp = eb.put_rule(
            Name=rule_name,
            EventBusName=bus_name,
            EventPattern=json.dumps({"source": ["my.app"]}),
        )
        assert "RuleArn" in resp
    run_test("EventBridge", "PutRule (event pattern)", t_put_rule_pattern)

    # PutRule (schedule)
    schedule_rule = f"test-sched-{uid()}"
    def t_put_rule_schedule():
        resp = eb.put_rule(
            Name=schedule_rule,
            EventBusName=bus_name,
            ScheduleExpression="rate(5 minutes)",
        )
        assert "RuleArn" in resp
    run_test("EventBridge", "PutRule (schedule)", t_put_rule_schedule)

    # ListRules
    def t_list_rules():
        resp = eb.list_rules(EventBusName=bus_name)
        names = [r["Name"] for r in resp.get("Rules", [])]
        assert rule_name in names
    run_test("EventBridge", "ListRules", t_list_rules)

    # DescribeRule
    def t_describe_rule():
        resp = eb.describe_rule(Name=rule_name, EventBusName=bus_name)
        assert resp.get("Name") == rule_name
    run_test("EventBridge", "DescribeRule", t_describe_rule)

    # PutTargets
    eb_queue_name = f"eb-target-{uid()}"
    eb_queue_url = None
    eb_queue_arn = None
    def t_put_targets():
        nonlocal eb_queue_url, eb_queue_arn
        q = sqs.create_queue(QueueName=eb_queue_name)
        eb_queue_url = q["QueueUrl"]
        attrs = sqs.get_queue_attributes(QueueUrl=eb_queue_url, AttributeNames=["QueueArn"])
        eb_queue_arn = attrs["Attributes"]["QueueArn"]
        resp = eb.put_targets(
            Rule=rule_name,
            EventBusName=bus_name,
            Targets=[{"Id": "sqs-target", "Arn": eb_queue_arn}],
        )
        assert resp.get("FailedEntryCount", 1) == 0
    run_test("EventBridge", "PutTargets", t_put_targets)

    # ListTargetsByRule
    def t_list_targets():
        resp = eb.list_targets_by_rule(Rule=rule_name, EventBusName=bus_name)
        targets = resp.get("Targets", [])
        assert len(targets) > 0, "No targets found"
    run_test("EventBridge", "ListTargetsByRule", t_list_targets)

    # PutEvents (matching)
    def t_put_events():
        resp = eb.put_events(
            Entries=[
                {
                    "Source": "my.app",
                    "DetailType": "TestEvent",
                    "Detail": json.dumps({"key": "value"}),
                    "EventBusName": bus_name,
                }
            ]
        )
        assert resp.get("FailedEntryCount", 1) == 0
    run_test("EventBridge", "PutEvents (matching)", t_put_events)

    # EventBridge -> SQS delivery
    def t_eb_sqs_delivery():
        if not eb_queue_url:
            raise AssertionError("No target queue")
        time.sleep(1)
        resp = sqs.receive_message(QueueUrl=eb_queue_url, MaxNumberOfMessages=10, WaitTimeSeconds=2)
        msgs = resp.get("Messages", [])
        assert len(msgs) > 0, "No events delivered to SQS"
    run_test("EventBridge", "EventBridge -> SQS delivery", t_eb_sqs_delivery)

    # PutEvents (non-matching)
    def t_put_events_nomatch():
        if eb_queue_url:
            sqs.purge_queue(QueueUrl=eb_queue_url)
            time.sleep(0.3)
        resp = eb.put_events(
            Entries=[
                {
                    "Source": "other.app",
                    "DetailType": "TestEvent",
                    "Detail": json.dumps({"key": "value"}),
                    "EventBusName": bus_name,
                }
            ]
        )
        assert resp.get("FailedEntryCount", 1) == 0
        time.sleep(0.5)
        if eb_queue_url:
            recv = sqs.receive_message(QueueUrl=eb_queue_url, MaxNumberOfMessages=1, WaitTimeSeconds=1)
            msgs = recv.get("Messages", [])
            assert len(msgs) == 0, f"Non-matching event was delivered ({len(msgs)} msgs)"
    run_test("EventBridge", "PutEvents (non-matching)", t_put_events_nomatch)

    # RemoveTargets
    def t_remove_targets():
        resp = eb.remove_targets(
            Rule=rule_name, EventBusName=bus_name, Ids=["sqs-target"]
        )
        assert resp.get("FailedEntryCount", 1) == 0
    run_test("EventBridge", "RemoveTargets", t_remove_targets)

    # TagResource
    def t_tag():
        eb.tag_resource(ResourceARN=bus_arn, Tags=[{"Key": "env", "Value": "test"}])
    run_test("EventBridge", "TagResource", t_tag)

    # ListTagsForResource
    def t_list_tags():
        resp = eb.list_tags_for_resource(ResourceARN=bus_arn)
        tags = resp.get("Tags", [])
        assert any(t["Key"] == "env" for t in tags)
    run_test("EventBridge", "ListTagsForResource", t_list_tags)

    # UntagResource
    def t_untag():
        eb.untag_resource(ResourceARN=bus_arn, TagKeys=["env"])
        resp = eb.list_tags_for_resource(ResourceARN=bus_arn)
        tags = resp.get("Tags", [])
        assert not any(t["Key"] == "env" for t in tags)
    run_test("EventBridge", "UntagResource", t_untag)

    # DeleteRule
    def t_delete_rule():
        tmp_rule = f"del-rule-{uid()}"
        eb.put_rule(
            Name=tmp_rule,
            EventBusName=bus_name,
            EventPattern=json.dumps({"source": ["x"]}),
        )
        eb.delete_rule(Name=tmp_rule, EventBusName=bus_name)
        resp = eb.list_rules(EventBusName=bus_name)
        names = [r["Name"] for r in resp.get("Rules", [])]
        assert tmp_rule not in names
    run_test("EventBridge", "DeleteRule", t_delete_rule)

    # DeleteEventBus
    def t_delete_bus():
        tmp_bus = f"del-bus-{uid()}"
        eb.create_event_bus(Name=tmp_bus)
        eb.delete_event_bus(Name=tmp_bus)
        resp = eb.list_event_buses()
        names = [b["Name"] for b in resp.get("EventBuses", [])]
        assert tmp_bus not in names
    run_test("EventBridge", "DeleteEventBus", t_delete_bus)

    # Cleanup
    try:
        eb.remove_targets(Rule=rule_name, EventBusName=bus_name, Ids=["sqs-target"])
    except Exception:
        pass
    for r in [rule_name, schedule_rule]:
        try:
            eb.delete_rule(Name=r, EventBusName=bus_name)
        except Exception:
            pass
    try:
        eb.delete_event_bus(Name=bus_name)
    except Exception:
        pass
    if eb_queue_url:
        try:
            sqs.delete_queue(QueueUrl=eb_queue_url)
        except Exception:
            pass


# ---------------------------------------------------------------------------
# IAM Tests
# ---------------------------------------------------------------------------
def test_iam():
    iam = client("iam")
    print("\n=== IAM Tests ===")

    user_name = f"test-user-{uid()}"
    role_name = f"test-role-{uid()}"
    policy_arn = None
    policy_name = f"test-policy-{uid()}"

    # CreateUser
    def t_create_user():
        resp = iam.create_user(UserName=user_name)
        assert resp["User"]["UserName"] == user_name
    run_test("IAM", "CreateUser", t_create_user)

    # GetUser
    def t_get_user():
        resp = iam.get_user(UserName=user_name)
        assert resp["User"]["UserName"] == user_name
    run_test("IAM", "GetUser", t_get_user)

    # ListUsers
    def t_list_users():
        resp = iam.list_users()
        names = [u["UserName"] for u in resp.get("Users", [])]
        assert user_name in names
    run_test("IAM", "ListUsers", t_list_users)

    # CreateAccessKey
    access_key_id = None
    def t_create_key():
        nonlocal access_key_id
        resp = iam.create_access_key(UserName=user_name)
        access_key_id = resp["AccessKey"]["AccessKeyId"]
        assert access_key_id
    run_test("IAM", "CreateAccessKey", t_create_key)

    # ListAccessKeys
    def t_list_keys():
        resp = iam.list_access_keys(UserName=user_name)
        ids = [k["AccessKeyId"] for k in resp.get("AccessKeyMetadata", [])]
        assert access_key_id in ids
    run_test("IAM", "ListAccessKeys", t_list_keys)

    # DeleteAccessKey
    def t_delete_key():
        if access_key_id:
            iam.delete_access_key(UserName=user_name, AccessKeyId=access_key_id)
    run_test("IAM", "DeleteAccessKey", t_delete_key)

    # CreateRole
    assume_role_doc = json.dumps({
        "Version": "2012-10-17",
        "Statement": [
            {
                "Effect": "Allow",
                "Principal": {"Service": "lambda.amazonaws.com"},
                "Action": "sts:AssumeRole",
            }
        ],
    })
    def t_create_role():
        resp = iam.create_role(
            RoleName=role_name,
            AssumeRolePolicyDocument=assume_role_doc,
        )
        assert resp["Role"]["RoleName"] == role_name
    run_test("IAM", "CreateRole", t_create_role)

    # GetRole
    def t_get_role():
        resp = iam.get_role(RoleName=role_name)
        assert resp["Role"]["RoleName"] == role_name
    run_test("IAM", "GetRole", t_get_role)

    # ListRoles
    def t_list_roles():
        resp = iam.list_roles()
        names = [r["RoleName"] for r in resp.get("Roles", [])]
        assert role_name in names
    run_test("IAM", "ListRoles", t_list_roles)

    # CreatePolicy
    def t_create_policy():
        nonlocal policy_arn
        resp = iam.create_policy(
            PolicyName=policy_name,
            PolicyDocument=json.dumps({
                "Version": "2012-10-17",
                "Statement": [
                    {
                        "Effect": "Allow",
                        "Action": "s3:GetObject",
                        "Resource": "*",
                    }
                ],
            }),
        )
        policy_arn = resp["Policy"]["Arn"]
        assert policy_arn
    run_test("IAM", "CreatePolicy", t_create_policy)

    # GetPolicy
    def t_get_policy():
        resp = iam.get_policy(PolicyArn=policy_arn)
        assert resp["Policy"]["Arn"] == policy_arn
    run_test("IAM", "GetPolicy", t_get_policy)

    # ListPolicies
    def t_list_policies():
        resp = iam.list_policies(Scope="Local")
        arns = [p["Arn"] for p in resp.get("Policies", [])]
        assert policy_arn in arns
    run_test("IAM", "ListPolicies", t_list_policies)

    # GetPolicyVersion
    def t_get_policy_version():
        resp = iam.get_policy_version(PolicyArn=policy_arn, VersionId="v1")
        assert resp["PolicyVersion"]
    run_test("IAM", "GetPolicyVersion", t_get_policy_version)

    # AttachRolePolicy
    def t_attach():
        iam.attach_role_policy(RoleName=role_name, PolicyArn=policy_arn)
    run_test("IAM", "AttachRolePolicy", t_attach)

    # ListAttachedRolePolicies
    def t_list_attached():
        resp = iam.list_attached_role_policies(RoleName=role_name)
        arns = [p["PolicyArn"] for p in resp.get("AttachedPolicies", [])]
        assert policy_arn in arns
    run_test("IAM", "ListAttachedRolePolicies", t_list_attached)

    # DetachRolePolicy
    def t_detach():
        iam.detach_role_policy(RoleName=role_name, PolicyArn=policy_arn)
        resp = iam.list_attached_role_policies(RoleName=role_name)
        arns = [p["PolicyArn"] for p in resp.get("AttachedPolicies", [])]
        assert policy_arn not in arns
    run_test("IAM", "DetachRolePolicy", t_detach)

    # DeletePolicy
    def t_delete_policy():
        iam.delete_policy(PolicyArn=policy_arn)
    run_test("IAM", "DeletePolicy", t_delete_policy)

    # DeleteRole
    def t_delete_role():
        iam.delete_role(RoleName=role_name)
    run_test("IAM", "DeleteRole", t_delete_role)

    # DeleteUser
    def t_delete_user():
        iam.delete_user(UserName=user_name)
    run_test("IAM", "DeleteUser", t_delete_user)


# ---------------------------------------------------------------------------
# STS Tests
# ---------------------------------------------------------------------------
def test_sts():
    sts = client("sts")
    iam = client("iam")
    print("\n=== STS Tests ===")

    # GetCallerIdentity
    def t_get_caller():
        resp = sts.get_caller_identity()
        assert "Account" in resp, f"No Account in response: {resp}"
        assert "Arn" in resp, f"No Arn in response"
    run_test("STS", "GetCallerIdentity", t_get_caller)

    # AssumeRole
    def t_assume_role():
        role_name = f"sts-role-{uid()}"
        assume_doc = json.dumps({
            "Version": "2012-10-17",
            "Statement": [{"Effect": "Allow", "Principal": {"AWS": "*"}, "Action": "sts:AssumeRole"}],
        })
        iam.create_role(RoleName=role_name, AssumeRolePolicyDocument=assume_doc)
        role = iam.get_role(RoleName=role_name)
        role_arn = role["Role"]["Arn"]

        resp = sts.assume_role(RoleArn=role_arn, RoleSessionName="test-session")
        creds = resp.get("Credentials", {})
        assert "AccessKeyId" in creds, f"No AccessKeyId: {creds}"
        assert "SecretAccessKey" in creds, f"No SecretAccessKey"
        assert creds["AccessKeyId"] != "testing", "Credentials should be unique"

        # Cleanup
        iam.delete_role(RoleName=role_name)
    run_test("STS", "AssumeRole", t_assume_role)


# ---------------------------------------------------------------------------
# SSM Tests
# ---------------------------------------------------------------------------
def test_ssm():
    ssm = client("ssm")
    print("\n=== SSM Tests ===")

    param_name = f"/test/param-{uid()}"

    # PutParameter (String)
    def t_put_string():
        resp = ssm.put_parameter(Name=param_name, Value="hello", Type="String")
        assert resp.get("Version", 0) >= 1
    run_test("SSM", "PutParameter (String)", t_put_string)

    # PutParameter (SecureString)
    secure_name = f"/test/secure-{uid()}"
    def t_put_secure():
        resp = ssm.put_parameter(Name=secure_name, Value="secret", Type="SecureString")
        assert resp.get("Version", 0) >= 1
    run_test("SSM", "PutParameter (SecureString)", t_put_secure)

    # PutParameter (overwrite)
    def t_put_overwrite():
        resp = ssm.put_parameter(
            Name=param_name, Value="updated", Type="String", Overwrite=True
        )
        assert resp.get("Version", 0) >= 2, f"Expected version >= 2: {resp}"
    run_test("SSM", "PutParameter (overwrite)", t_put_overwrite)

    # GetParameter
    def t_get_param():
        resp = ssm.get_parameter(Name=param_name)
        p = resp["Parameter"]
        assert p["Value"] == "updated", f"Expected 'updated', got '{p['Value']}'"
    run_test("SSM", "GetParameter", t_get_param)

    # GetParameter (WithDecryption)
    def t_get_decrypt():
        resp = ssm.get_parameter(Name=secure_name, WithDecryption=True)
        assert resp["Parameter"]["Value"] == "secret"
    run_test("SSM", "GetParameter (WithDecryption)", t_get_decrypt)

    # GetParameters (multiple)
    def t_get_params():
        resp = ssm.get_parameters(Names=[param_name, secure_name])
        assert len(resp.get("Parameters", [])) == 2
    run_test("SSM", "GetParameters (multiple)", t_get_params)

    # GetParameters (with invalid)
    def t_get_params_invalid():
        resp = ssm.get_parameters(Names=[param_name, "/nonexistent/param"])
        assert len(resp.get("Parameters", [])) >= 1
        assert len(resp.get("InvalidParameters", [])) >= 1
    run_test("SSM", "GetParameters (with invalid)", t_get_params_invalid)

    # GetParametersByPath (non-recursive)
    path_base = f"/path-test-{uid()}"
    def t_get_by_path():
        ssm.put_parameter(Name=f"{path_base}/a", Value="a", Type="String")
        ssm.put_parameter(Name=f"{path_base}/b", Value="b", Type="String")
        ssm.put_parameter(Name=f"{path_base}/sub/c", Value="c", Type="String")
        resp = ssm.get_parameters_by_path(Path=path_base)
        params = resp.get("Parameters", [])
        names = [p["Name"] for p in params]
        assert f"{path_base}/a" in names, f"Missing /a: {names}"
        assert f"{path_base}/b" in names, f"Missing /b: {names}"
        # Non-recursive should NOT include sub/c
        assert f"{path_base}/sub/c" not in names, f"sub/c should not be included non-recursively"
    run_test("SSM", "GetParametersByPath (non-recursive)", t_get_by_path)

    # GetParametersByPath (recursive)
    def t_get_by_path_recursive():
        resp = ssm.get_parameters_by_path(Path=path_base, Recursive=True)
        params = resp.get("Parameters", [])
        names = [p["Name"] for p in params]
        assert f"{path_base}/sub/c" in names, f"Missing sub/c in recursive: {names}"
    run_test("SSM", "GetParametersByPath (recursive)", t_get_by_path_recursive)

    # GetParametersByPath (pagination)
    def t_get_by_path_pagination():
        pag_base = f"/pag-test-{uid()}"
        for i in range(12):
            ssm.put_parameter(Name=f"{pag_base}/p{i:02d}", Value=f"v{i}", Type="String")
        all_params = []
        token = None
        while True:
            kwargs = {"Path": pag_base, "MaxResults": 5}
            if token:
                kwargs["NextToken"] = token
            resp = ssm.get_parameters_by_path(**kwargs)
            all_params.extend(resp.get("Parameters", []))
            token = resp.get("NextToken")
            if not token:
                break
        assert len(all_params) == 12, f"Expected 12 params via pagination, got {len(all_params)}"
    run_test("SSM", "GetParametersByPath (pagination)", t_get_by_path_pagination)

    # DescribeParameters
    def t_describe():
        resp = ssm.describe_parameters()
        params = resp.get("Parameters", [])
        assert len(params) > 0, "No parameters described"
    run_test("SSM", "DescribeParameters", t_describe)

    # GetParameterHistory
    def t_history():
        resp = ssm.get_parameter_history(Name=param_name)
        history = resp.get("Parameters", [])
        assert len(history) >= 2, f"Expected >= 2 history entries, got {len(history)}"
    run_test("SSM", "GetParameterHistory", t_history)

    # AddTagsToResource
    def t_add_tags():
        ssm.add_tags_to_resource(
            ResourceType="Parameter",
            ResourceId=param_name,
            Tags=[{"Key": "env", "Value": "test"}],
        )
    run_test("SSM", "AddTagsToResource", t_add_tags)

    # ListTagsForResource
    def t_list_tags():
        resp = ssm.list_tags_for_resource(
            ResourceType="Parameter", ResourceId=param_name
        )
        tags = resp.get("TagList", [])
        assert any(t["Key"] == "env" for t in tags), f"Tag not found: {tags}"
    run_test("SSM", "ListTagsForResource", t_list_tags)

    # RemoveTagsFromResource
    def t_remove_tags():
        ssm.remove_tags_from_resource(
            ResourceType="Parameter",
            ResourceId=param_name,
            TagKeys=["env"],
        )
        resp = ssm.list_tags_for_resource(
            ResourceType="Parameter", ResourceId=param_name
        )
        tags = resp.get("TagList", [])
        assert not any(t["Key"] == "env" for t in tags), "Tag not removed"
    run_test("SSM", "RemoveTagsFromResource", t_remove_tags)

    # DeleteParameter
    def t_delete_param():
        ssm.delete_parameter(Name=secure_name)
        try:
            ssm.get_parameter(Name=secure_name)
            raise AssertionError("Parameter still exists after delete")
        except ClientError as e:
            assert "ParameterNotFound" in str(e)
    run_test("SSM", "DeleteParameter", t_delete_param)

    # DeleteParameters
    def t_delete_params():
        n1 = f"/test/del-{uid()}"
        n2 = f"/test/del-{uid()}"
        ssm.put_parameter(Name=n1, Value="x", Type="String")
        ssm.put_parameter(Name=n2, Value="y", Type="String")
        resp = ssm.delete_parameters(Names=[n1, n2])
        deleted = resp.get("DeletedParameters", [])
        assert len(deleted) == 2, f"Expected 2 deleted: {deleted}"
    run_test("SSM", "DeleteParameters", t_delete_params)


# ---------------------------------------------------------------------------
# Cross-Service Tests
# ---------------------------------------------------------------------------
def test_cross_service():
    print("\n=== Cross-Service Tests ===")
    sns = client("sns")
    sqs = client("sqs")
    eb = client("events")

    # SNS -> SQS fan-out (dedicated test)
    def t_fanout():
        q1_url = sqs.create_queue(QueueName=f"fanout1-{uid()}")["QueueUrl"]
        q2_url = sqs.create_queue(QueueName=f"fanout2-{uid()}")["QueueUrl"]
        q1_arn = sqs.get_queue_attributes(QueueUrl=q1_url, AttributeNames=["QueueArn"])["Attributes"]["QueueArn"]
        q2_arn = sqs.get_queue_attributes(QueueUrl=q2_url, AttributeNames=["QueueArn"])["Attributes"]["QueueArn"]

        topic_arn = sns.create_topic(Name=f"fanout-{uid()}")["TopicArn"]
        sns.subscribe(TopicArn=topic_arn, Protocol="sqs", Endpoint=q1_arn)
        sns.subscribe(TopicArn=topic_arn, Protocol="sqs", Endpoint=q2_arn)

        sns.publish(TopicArn=topic_arn, Message="fanout-msg")
        time.sleep(1.5)

        r1 = sqs.receive_message(QueueUrl=q1_url, MaxNumberOfMessages=10, WaitTimeSeconds=2)
        r2 = sqs.receive_message(QueueUrl=q2_url, MaxNumberOfMessages=10, WaitTimeSeconds=2)
        assert len(r1.get("Messages", [])) > 0, "Queue 1 got no messages"
        assert len(r2.get("Messages", [])) > 0, "Queue 2 got no messages"

        # Cleanup
        sns.delete_topic(TopicArn=topic_arn)
        sqs.delete_queue(QueueUrl=q1_url)
        sqs.delete_queue(QueueUrl=q2_url)
    run_test("Cross-Service", "SNS -> SQS fan-out (2 queues)", t_fanout)

    # EventBridge -> SQS delivery
    def t_eb_sqs():
        q_url = sqs.create_queue(QueueName=f"eb-xsvc-{uid()}")["QueueUrl"]
        q_arn = sqs.get_queue_attributes(QueueUrl=q_url, AttributeNames=["QueueArn"])["Attributes"]["QueueArn"]

        bus_name = f"xsvc-bus-{uid()}"
        eb.create_event_bus(Name=bus_name)
        rule_name = f"xsvc-rule-{uid()}"
        eb.put_rule(
            Name=rule_name,
            EventBusName=bus_name,
            EventPattern=json.dumps({"source": ["xsvc.test"]}),
        )
        eb.put_targets(
            Rule=rule_name,
            EventBusName=bus_name,
            Targets=[{"Id": "t1", "Arn": q_arn}],
        )

        eb.put_events(
            Entries=[{
                "Source": "xsvc.test",
                "DetailType": "Test",
                "Detail": json.dumps({"foo": "bar"}),
                "EventBusName": bus_name,
            }]
        )
        time.sleep(1.5)
        resp = sqs.receive_message(QueueUrl=q_url, MaxNumberOfMessages=10, WaitTimeSeconds=2)
        assert len(resp.get("Messages", [])) > 0, "No events delivered"

        # Cleanup
        eb.remove_targets(Rule=rule_name, EventBusName=bus_name, Ids=["t1"])
        eb.delete_rule(Name=rule_name, EventBusName=bus_name)
        eb.delete_event_bus(Name=bus_name)
        sqs.delete_queue(QueueUrl=q_url)
    run_test("Cross-Service", "EventBridge -> SQS delivery", t_eb_sqs)

    # EventBridge -> SNS -> SQS chain
    def t_eb_sns_sqs():
        q_url = sqs.create_queue(QueueName=f"chain-q-{uid()}")["QueueUrl"]
        q_arn = sqs.get_queue_attributes(QueueUrl=q_url, AttributeNames=["QueueArn"])["Attributes"]["QueueArn"]

        topic_arn = sns.create_topic(Name=f"chain-t-{uid()}")["TopicArn"]
        sns.subscribe(TopicArn=topic_arn, Protocol="sqs", Endpoint=q_arn)

        bus_name = f"chain-bus-{uid()}"
        eb.create_event_bus(Name=bus_name)
        rule_name = f"chain-rule-{uid()}"
        eb.put_rule(
            Name=rule_name,
            EventBusName=bus_name,
            EventPattern=json.dumps({"source": ["chain.test"]}),
        )
        eb.put_targets(
            Rule=rule_name,
            EventBusName=bus_name,
            Targets=[{"Id": "sns-t", "Arn": topic_arn}],
        )

        eb.put_events(
            Entries=[{
                "Source": "chain.test",
                "DetailType": "ChainTest",
                "Detail": json.dumps({"chain": True}),
                "EventBusName": bus_name,
            }]
        )
        time.sleep(2)
        resp = sqs.receive_message(QueueUrl=q_url, MaxNumberOfMessages=10, WaitTimeSeconds=3)
        msgs = resp.get("Messages", [])
        assert len(msgs) > 0, "Chain delivery failed: no messages in SQS"

        # Cleanup
        eb.remove_targets(Rule=rule_name, EventBusName=bus_name, Ids=["sns-t"])
        eb.delete_rule(Name=rule_name, EventBusName=bus_name)
        eb.delete_event_bus(Name=bus_name)
        sns.delete_topic(TopicArn=topic_arn)
        sqs.delete_queue(QueueUrl=q_url)
    run_test("Cross-Service", "EventBridge -> SNS -> SQS chain", t_eb_sns_sqs)


# ---------------------------------------------------------------------------
# Report
# ---------------------------------------------------------------------------
def print_report():
    print("\n" + "=" * 70)
    print("COMPATIBILITY REPORT")
    print("=" * 70)

    total_pass = 0
    total_fail = 0

    for service in ["SQS", "SNS", "EventBridge", "IAM", "STS", "SSM", "Cross-Service"]:
        tests = results.get(service, [])
        if not tests:
            continue
        passed = sum(1 for _, p, _ in tests if p)
        failed = sum(1 for _, p, _ in tests if not p)
        total_pass += passed
        total_fail += failed
        pct = (passed / len(tests) * 100) if tests else 0
        print(f"\n  {service:20s}  {passed:3d}/{len(tests):3d} passed  ({pct:5.1f}%)")
        if failed > 0:
            for name, p, err in tests:
                if not p:
                    print(f"    FAIL: {name}")
                    if err:
                        for line in err.strip().split("\n")[:3]:
                            print(f"          {line}")

    total = total_pass + total_fail
    pct = (total_pass / total * 100) if total else 0
    print(f"\n{'─' * 70}")
    print(f"  TOTAL: {total_pass}/{total} passed ({pct:.1f}%)")
    if total_fail > 0:
        print(f"  {total_fail} FAILURES")
    print("=" * 70)

    return total_fail


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
if __name__ == "__main__":
    print("FakeCloud Compatibility Test Suite")
    print(f"Endpoint: {ENDPOINT}")
    print(f"Region: {REGION}")

    test_sqs()
    test_sns()
    test_eventbridge()
    test_iam()
    test_sts()
    test_ssm()
    test_cross_service()

    failures = print_report()
    sys.exit(1 if failures else 0)
