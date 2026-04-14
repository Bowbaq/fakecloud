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

#[tokio::test]
async fn dynamodb_query_and_conditional_put() {
    let backend = Backend::from_env().await;
    let ddb = backend.dynamodb().await;
    let table = unique_name("ddb-query");

    // Composite key: hash "pk", range "sk".
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
        .attribute_definitions(
            AttributeDefinition::builder()
                .attribute_name("sk")
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
        .key_schema(
            KeySchemaElement::builder()
                .attribute_name("sk")
                .key_type(KeyType::Range)
                .build()
                .unwrap(),
        )
        .send()
        .await
        .expect("create_table");

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

    // Insert three rows in the same partition, different sort keys.
    for sk in ["001", "002", "003"] {
        ddb.put_item()
            .table_name(&table)
            .item("pk", AttributeValue::S("user-1".into()))
            .item("sk", AttributeValue::S(sk.into()))
            .send()
            .await
            .expect("put_item");
    }

    // Query the partition.
    let query = ddb
        .query()
        .table_name(&table)
        .key_condition_expression("pk = :pk")
        .expression_attribute_values(":pk", AttributeValue::S("user-1".into()))
        .send()
        .await
        .expect("query");
    let items = query.items();
    assert_eq!(items.len(), 3, "expected 3 rows, got {}", items.len());
    // Default sort order is ascending on sk.
    let sks: Vec<String> = items
        .iter()
        .filter_map(|i| i.get("sk").and_then(|v| v.as_s().ok()).cloned())
        .collect();
    assert_eq!(
        sks,
        vec!["001".to_string(), "002".to_string(), "003".to_string()]
    );

    // Conditional put: attribute_not_exists(pk) should succeed for a new
    // row and fail with ConditionalCheckFailedException for an existing one.
    ddb.put_item()
        .table_name(&table)
        .item("pk", AttributeValue::S("user-2".into()))
        .item("sk", AttributeValue::S("001".into()))
        .condition_expression("attribute_not_exists(pk)")
        .send()
        .await
        .expect("conditional put first time");

    let err = ddb
        .put_item()
        .table_name(&table)
        .item("pk", AttributeValue::S("user-1".into()))
        .item("sk", AttributeValue::S("001".into()))
        .condition_expression("attribute_not_exists(pk)")
        .send()
        .await
        .expect_err("conditional put should fail second time");
    let code = err
        .into_service_error()
        .meta()
        .code()
        .unwrap_or_default()
        .to_string();
    assert_eq!(
        code, "ConditionalCheckFailedException",
        "expected ConditionalCheckFailedException, got {code:?}"
    );

    ddb.delete_table()
        .table_name(&table)
        .send()
        .await
        .expect("delete_table");
}
