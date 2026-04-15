mod helpers;

use std::collections::HashMap;

use aws_sdk_dynamodb::primitives::Blob;
use aws_sdk_dynamodb::types::{
    AttributeDefinition, AttributeValue, BillingMode, GlobalSecondaryIndex, KeySchemaElement,
    KeyType, LocalSecondaryIndex, Projection, ProjectionType, ProvisionedThroughput,
    ScalarAttributeType, TimeToLiveSpecification,
};
use helpers::TestServer;

/// Full round-trip: create tables, populate items across every attribute
/// type, configure TTL + tags + GSIs + LSIs, restart, and assert everything
/// survives including item contents.
#[tokio::test]
async fn persistence_round_trip_tables_and_items() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let client = server.dynamodb_client().await;

    // Table with composite key, LSI, GSI, PROVISIONED billing, TTL, tags.
    client
        .create_table()
        .table_name("users")
        .key_schema(
            KeySchemaElement::builder()
                .attribute_name("pk")
                .key_type(KeyType::Hash)
                .build()
                .unwrap(),
        )
        .key_schema(
            KeySchemaElement::builder()
                .attribute_name("sk")
                .key_type(KeyType::Range)
                .build()
                .unwrap(),
        )
        .attribute_definitions(
            AttributeDefinition::builder()
                .attribute_name("pk")
                .attribute_type(ScalarAttributeType::S)
                .build()
                .unwrap(),
        )
        .attribute_definitions(
            AttributeDefinition::builder()
                .attribute_name("sk")
                .attribute_type(ScalarAttributeType::S)
                .build()
                .unwrap(),
        )
        .attribute_definitions(
            AttributeDefinition::builder()
                .attribute_name("email")
                .attribute_type(ScalarAttributeType::S)
                .build()
                .unwrap(),
        )
        .attribute_definitions(
            AttributeDefinition::builder()
                .attribute_name("created_at")
                .attribute_type(ScalarAttributeType::N)
                .build()
                .unwrap(),
        )
        .global_secondary_indexes(
            GlobalSecondaryIndex::builder()
                .index_name("by_email")
                .key_schema(
                    KeySchemaElement::builder()
                        .attribute_name("email")
                        .key_type(KeyType::Hash)
                        .build()
                        .unwrap(),
                )
                .projection(
                    Projection::builder()
                        .projection_type(ProjectionType::All)
                        .build(),
                )
                .provisioned_throughput(
                    ProvisionedThroughput::builder()
                        .read_capacity_units(5)
                        .write_capacity_units(5)
                        .build()
                        .unwrap(),
                )
                .build()
                .unwrap(),
        )
        .local_secondary_indexes(
            LocalSecondaryIndex::builder()
                .index_name("by_created_at")
                .key_schema(
                    KeySchemaElement::builder()
                        .attribute_name("pk")
                        .key_type(KeyType::Hash)
                        .build()
                        .unwrap(),
                )
                .key_schema(
                    KeySchemaElement::builder()
                        .attribute_name("created_at")
                        .key_type(KeyType::Range)
                        .build()
                        .unwrap(),
                )
                .projection(
                    Projection::builder()
                        .projection_type(ProjectionType::KeysOnly)
                        .build(),
                )
                .build()
                .unwrap(),
        )
        .provisioned_throughput(
            ProvisionedThroughput::builder()
                .read_capacity_units(10)
                .write_capacity_units(10)
                .build()
                .unwrap(),
        )
        .send()
        .await
        .unwrap();

    // PAY_PER_REQUEST table, minimal schema, to exercise billing_mode persistence.
    client
        .create_table()
        .table_name("events")
        .billing_mode(BillingMode::PayPerRequest)
        .key_schema(
            KeySchemaElement::builder()
                .attribute_name("id")
                .key_type(KeyType::Hash)
                .build()
                .unwrap(),
        )
        .attribute_definitions(
            AttributeDefinition::builder()
                .attribute_name("id")
                .attribute_type(ScalarAttributeType::S)
                .build()
                .unwrap(),
        )
        .send()
        .await
        .unwrap();

    // TTL on users table.
    client
        .update_time_to_live()
        .table_name("users")
        .time_to_live_specification(
            TimeToLiveSpecification::builder()
                .attribute_name("expires_at")
                .enabled(true)
                .build()
                .unwrap(),
        )
        .send()
        .await
        .unwrap();

    // Tags on users.
    client
        .tag_resource()
        .resource_arn("arn:aws:dynamodb:us-east-1:123456789012:table/users")
        .tags(
            aws_sdk_dynamodb::types::Tag::builder()
                .key("env")
                .value("prod")
                .build()
                .unwrap(),
        )
        .tags(
            aws_sdk_dynamodb::types::Tag::builder()
                .key("team")
                .value("platform")
                .build()
                .unwrap(),
        )
        .send()
        .await
        .unwrap();

    // Insert items exercising every AttributeValue variant.
    let mut item1: HashMap<String, AttributeValue> = HashMap::new();
    item1.insert("pk".into(), AttributeValue::S("user#1".into()));
    item1.insert("sk".into(), AttributeValue::S("profile".into()));
    item1.insert("email".into(), AttributeValue::S("a@example.com".into()));
    item1.insert("created_at".into(), AttributeValue::N("1700000000".into()));
    item1.insert("age".into(), AttributeValue::N("30".into()));
    item1.insert("active".into(), AttributeValue::Bool(true));
    item1.insert("middle_name".into(), AttributeValue::Null(true));
    item1.insert(
        "tags".into(),
        AttributeValue::Ss(vec!["admin".into(), "beta".into()]),
    );
    item1.insert(
        "scores".into(),
        AttributeValue::Ns(vec!["1".into(), "2".into(), "3".into()]),
    );
    item1.insert(
        "blobs".into(),
        AttributeValue::Bs(vec![Blob::new(vec![1u8, 2, 3]), Blob::new(vec![4u8, 5, 6])]),
    );
    item1.insert("raw".into(), AttributeValue::B(Blob::new(vec![9u8; 16])));
    item1.insert(
        "history".into(),
        AttributeValue::L(vec![
            AttributeValue::S("created".into()),
            AttributeValue::N("1".into()),
        ]),
    );
    let mut nested: HashMap<String, AttributeValue> = HashMap::new();
    nested.insert("city".into(), AttributeValue::S("SP".into()));
    nested.insert("zip".into(), AttributeValue::S("01000".into()));
    item1.insert("address".into(), AttributeValue::M(nested));

    client
        .put_item()
        .table_name("users")
        .set_item(Some(item1.clone()))
        .send()
        .await
        .unwrap();

    // Second simpler item.
    let mut item2: HashMap<String, AttributeValue> = HashMap::new();
    item2.insert("pk".into(), AttributeValue::S("user#2".into()));
    item2.insert("sk".into(), AttributeValue::S("profile".into()));
    item2.insert("email".into(), AttributeValue::S("b@example.com".into()));
    item2.insert("created_at".into(), AttributeValue::N("1700000100".into()));
    client
        .put_item()
        .table_name("users")
        .set_item(Some(item2))
        .send()
        .await
        .unwrap();

    // Item in the PAY_PER_REQUEST table.
    let mut ev: HashMap<String, AttributeValue> = HashMap::new();
    ev.insert("id".into(), AttributeValue::S("evt-1".into()));
    ev.insert("payload".into(), AttributeValue::S("hello".into()));
    client
        .put_item()
        .table_name("events")
        .set_item(Some(ev))
        .send()
        .await
        .unwrap();

    // Restart the server with the same data path — in-memory state is
    // rebuilt exclusively from the persisted snapshot on disk.
    server.restart().await;
    let client = server.dynamodb_client().await;

    // Tables still listed.
    let list = client.list_tables().send().await.unwrap();
    let mut table_names: Vec<String> = list.table_names().to_vec();
    table_names.sort();
    assert_eq!(table_names, vec!["events".to_string(), "users".to_string()]);

    // Describe users: provisioned throughput, GSI, LSI, billing mode.
    let desc = client
        .describe_table()
        .table_name("users")
        .send()
        .await
        .unwrap();
    let t = desc.table().unwrap();
    assert_eq!(
        t.provisioned_throughput().unwrap().read_capacity_units(),
        Some(10)
    );
    assert_eq!(t.global_secondary_indexes().len(), 1);
    assert_eq!(t.local_secondary_indexes().len(), 1);
    assert_eq!(
        t.global_secondary_indexes()
            .first()
            .unwrap()
            .index_name()
            .unwrap(),
        "by_email"
    );

    let ev_desc = client
        .describe_table()
        .table_name("events")
        .send()
        .await
        .unwrap();
    assert_eq!(
        ev_desc
            .table()
            .unwrap()
            .billing_mode_summary()
            .and_then(|b| b.billing_mode()),
        Some(&BillingMode::PayPerRequest),
    );

    // TTL config survives.
    let ttl = client
        .describe_time_to_live()
        .table_name("users")
        .send()
        .await
        .unwrap();
    let ttl_spec = ttl.time_to_live_description().unwrap();
    assert_eq!(ttl_spec.attribute_name(), Some("expires_at"));

    // Tags survive.
    let tags = client
        .list_tags_of_resource()
        .resource_arn("arn:aws:dynamodb:us-east-1:123456789012:table/users")
        .send()
        .await
        .unwrap();
    let tag_map: HashMap<String, String> = tags
        .tags()
        .iter()
        .map(|t| (t.key().to_string(), t.value().to_string()))
        .collect();
    assert_eq!(tag_map.get("env").map(String::as_str), Some("prod"));
    assert_eq!(tag_map.get("team").map(String::as_str), Some("platform"));

    // Items survive with every attribute variant intact.
    let got = client
        .get_item()
        .table_name("users")
        .key("pk", AttributeValue::S("user#1".into()))
        .key("sk", AttributeValue::S("profile".into()))
        .send()
        .await
        .unwrap();
    let got_item = got.item().unwrap();
    assert_eq!(got_item, &item1);

    // Scan returns both users.
    let scan = client.scan().table_name("users").send().await.unwrap();
    assert_eq!(scan.count(), 2);

    // PAY_PER_REQUEST item survives.
    let ev_got = client
        .get_item()
        .table_name("events")
        .key("id", AttributeValue::S("evt-1".into()))
        .send()
        .await
        .unwrap();
    assert_eq!(
        ev_got.item().unwrap().get("payload"),
        Some(&AttributeValue::S("hello".into())),
    );

    // Mutations after restart still persist. Delete an item, restart again,
    // and confirm it stays deleted.
    client
        .delete_item()
        .table_name("users")
        .key("pk", AttributeValue::S("user#2".into()))
        .key("sk", AttributeValue::S("profile".into()))
        .send()
        .await
        .unwrap();

    server.restart().await;
    let client = server.dynamodb_client().await;
    let scan = client.scan().table_name("users").send().await.unwrap();
    assert_eq!(scan.count(), 1);
}

/// Dropping a table + restart: the table is gone, not resurrected by a
/// stale snapshot.
#[tokio::test]
async fn persistence_delete_table_survives_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let client = server.dynamodb_client().await;

    client
        .create_table()
        .table_name("ephemeral")
        .billing_mode(BillingMode::PayPerRequest)
        .key_schema(
            KeySchemaElement::builder()
                .attribute_name("id")
                .key_type(KeyType::Hash)
                .build()
                .unwrap(),
        )
        .attribute_definitions(
            AttributeDefinition::builder()
                .attribute_name("id")
                .attribute_type(ScalarAttributeType::S)
                .build()
                .unwrap(),
        )
        .send()
        .await
        .unwrap();

    client
        .delete_table()
        .table_name("ephemeral")
        .send()
        .await
        .unwrap();

    server.restart().await;
    let client = server.dynamodb_client().await;
    let list = client.list_tables().send().await.unwrap();
    assert!(list.table_names().is_empty(), "table should not resurrect");
}
