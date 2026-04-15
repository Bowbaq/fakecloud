mod helpers;

use aws_sdk_kinesis::primitives::Blob;
use aws_sdk_kinesis::types::ShardIteratorType;
use helpers::TestServer;

/// Stream, tags, shards, and previously-put records all survive a restart.
#[tokio::test]
async fn persistence_round_trip_stream_and_records() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let k = server.kinesis_client().await;

    k.create_stream()
        .stream_name("events")
        .shard_count(2)
        .send()
        .await
        .unwrap();

    k.add_tags_to_stream()
        .stream_name("events")
        .tags("env", "prod")
        .send()
        .await
        .unwrap();

    for i in 0..3u32 {
        k.put_record()
            .stream_name("events")
            .partition_key(format!("pk-{i}"))
            .data(Blob::new(format!("record-{i}").into_bytes()))
            .send()
            .await
            .unwrap();
    }

    server.restart().await;
    let k = server.kinesis_client().await;

    // Stream survives.
    let desc = k
        .describe_stream()
        .stream_name("events")
        .send()
        .await
        .unwrap();
    let sd = desc.stream_description().unwrap();
    assert_eq!(sd.stream_name(), "events");
    assert_eq!(sd.shards().len(), 2);

    // Tags survive.
    let tags = k
        .list_tags_for_stream()
        .stream_name("events")
        .send()
        .await
        .unwrap();
    assert!(tags
        .tags()
        .iter()
        .any(|t| t.key() == "env" && t.value() == Some("prod")));

    // Fetch records from both shards and make sure all three pre-restart
    // payloads come back.
    let mut seen: Vec<String> = Vec::new();
    for shard in sd.shards() {
        let it = k
            .get_shard_iterator()
            .stream_name("events")
            .shard_id(shard.shard_id())
            .shard_iterator_type(ShardIteratorType::TrimHorizon)
            .send()
            .await
            .unwrap()
            .shard_iterator
            .unwrap();
        let recs = k.get_records().shard_iterator(it).send().await.unwrap();
        for r in recs.records() {
            seen.push(String::from_utf8(r.data().as_ref().to_vec()).unwrap());
        }
    }
    seen.sort();
    assert_eq!(seen, vec!["record-0", "record-1", "record-2"]);
}

/// DeleteStream durability.
#[tokio::test]
async fn persistence_delete_stream_survives_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let k = server.kinesis_client().await;

    k.create_stream()
        .stream_name("ephemeral")
        .shard_count(1)
        .send()
        .await
        .unwrap();
    k.delete_stream()
        .stream_name("ephemeral")
        .send()
        .await
        .unwrap();

    server.restart().await;
    let k = server.kinesis_client().await;

    let streams = k.list_streams().send().await.unwrap();
    assert!(!streams.stream_names().iter().any(|s| s == "ephemeral"));
}

/// Retention period + encryption state survive restart.
#[tokio::test]
async fn persistence_retention_and_encryption_survive_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let k = server.kinesis_client().await;

    k.create_stream()
        .stream_name("secure")
        .shard_count(1)
        .send()
        .await
        .unwrap();
    k.increase_stream_retention_period()
        .stream_name("secure")
        .retention_period_hours(48)
        .send()
        .await
        .unwrap();
    k.start_stream_encryption()
        .stream_name("secure")
        .encryption_type(aws_sdk_kinesis::types::EncryptionType::Kms)
        .key_id("alias/aws/kinesis")
        .send()
        .await
        .unwrap();

    server.restart().await;
    let k = server.kinesis_client().await;

    let sd = k
        .describe_stream()
        .stream_name("secure")
        .send()
        .await
        .unwrap()
        .stream_description
        .unwrap();
    assert_eq!(sd.retention_period_hours(), 48);
    assert_eq!(
        sd.encryption_type(),
        Some(&aws_sdk_kinesis::types::EncryptionType::Kms)
    );

    // describe_stream doesn't surface KeyId, but describe_stream_summary
    // does -- verify through it.
    let summary = k
        .describe_stream_summary()
        .stream_name("secure")
        .send()
        .await
        .unwrap()
        .stream_description_summary
        .unwrap();
    assert_eq!(summary.key_id(), Some("alias/aws/kinesis"));
}
