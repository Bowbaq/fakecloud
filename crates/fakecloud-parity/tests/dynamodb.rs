mod common;

use aws_sdk_dynamodb::types::{
    AttributeDefinition, AttributeValue, BillingMode, KeySchemaElement, KeyType,
    ScalarAttributeType, TableStatus,
};
use common::{retry, unique_name, Backend};

#[tokio::test]
async fn dynamodb_create_put_get_delete() {
    let backend = Backend::from_env().await;
    let ddb = backend.dynamodb().await;
    let table = unique_name("ddb");

    ddb.create_table()
        .table_name(&table)
        .billing_mode(BillingMode::PayPerRequest)
        .attribute_definitions(
            AttributeDefinition::builder()
                .attribute_name("pk")
                .attribute_type(ScalarAttributeType::S)
                .build()
                .unwrap(),
        )
        .key_schema(
            KeySchemaElement::builder()
                .attribute_name("pk")
                .key_type(KeyType::Hash)
                .build()
                .unwrap(),
        )
        .send()
        .await
        .expect("create_table");

    // Wait until the table is ACTIVE. Real AWS takes several seconds.
    retry(60, 1000, || async {
        let desc = ddb
            .describe_table()
            .table_name(&table)
            .send()
            .await
            .map_err(|e| format!("describe_table: {e}"))?;
        let status = desc
            .table()
            .and_then(|t| t.table_status())
            .cloned()
            .ok_or_else(|| "no table_status".to_string())?;
        if status == TableStatus::Active {
            Ok(())
        } else {
            Err(format!("table status = {status:?}"))
        }
    })
    .await
    .expect("table did not become ACTIVE");

    // Put -> Get.
    ddb.put_item()
        .table_name(&table)
        .item("pk", AttributeValue::S("alpha".into()))
        .item("value", AttributeValue::N("42".into()))
        .send()
        .await
        .expect("put_item");

    let got = ddb
        .get_item()
        .table_name(&table)
        .key("pk", AttributeValue::S("alpha".into()))
        .consistent_read(true)
        .send()
        .await
        .expect("get_item");
    let item = got.item().expect("item returned");
    assert_eq!(
        item.get("value")
            .and_then(|v| v.as_n().ok())
            .map(String::as_str),
        Some("42")
    );

    // Delete table.
    ddb.delete_table()
        .table_name(&table)
        .send()
        .await
        .expect("delete_table");
}
