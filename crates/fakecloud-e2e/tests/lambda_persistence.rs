mod helpers;

use std::io::Write;

use aws_sdk_lambda::primitives::Blob;
use helpers::TestServer;

fn make_python_zip() -> Vec<u8> {
    let buf = Vec::new();
    let cursor = std::io::Cursor::new(buf);
    let mut writer = zip::ZipWriter::new(cursor);
    let options = zip::write::SimpleFileOptions::default();
    writer.start_file("index.py", options).unwrap();
    writer
        .write_all(b"def handler(event, context):\n    return {\"statusCode\": 200}\n")
        .unwrap();
    let cursor = writer.finish().unwrap();
    cursor.into_inner()
}

/// Function + event source mapping survive a restart.
#[tokio::test]
async fn persistence_round_trip_function_and_esm() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let client = server.lambda_client().await;

    client
        .create_function()
        .function_name("persist-func")
        .runtime(aws_sdk_lambda::types::Runtime::Python312)
        .role("arn:aws:iam::123456789012:role/test-role")
        .handler("index.handler")
        .code(
            aws_sdk_lambda::types::FunctionCode::builder()
                .zip_file(Blob::new(make_python_zip()))
                .build(),
        )
        .send()
        .await
        .unwrap();

    let esm = client
        .create_event_source_mapping()
        .function_name("persist-func")
        .event_source_arn("arn:aws:sqs:us-east-1:123456789012:persist-queue")
        .batch_size(5)
        .send()
        .await
        .unwrap();
    let esm_uuid = esm.uuid().unwrap().to_string();

    drop(client);
    server.restart().await;
    let client = server.lambda_client().await;

    // Function survived.
    let func = client
        .get_function()
        .function_name("persist-func")
        .send()
        .await
        .unwrap();
    let config = func.configuration().unwrap();
    assert_eq!(config.function_name().unwrap(), "persist-func");
    assert_eq!(config.runtime().unwrap().as_str(), "python3.12");

    // ESM survived.
    let esm_get = client
        .get_event_source_mapping()
        .uuid(&esm_uuid)
        .send()
        .await
        .unwrap();
    assert_eq!(esm_get.batch_size(), Some(5));

    // ListFunctions returns the function.
    let listed = client.list_functions().send().await.unwrap();
    assert!(listed
        .functions()
        .iter()
        .any(|f| f.function_name() == Some("persist-func")));
}

/// Invocations introspection buffer resets on restart.
#[tokio::test]
async fn persistence_invocations_not_persisted() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let client = server.lambda_client().await;

    client
        .create_function()
        .function_name("invoke-func")
        .runtime(aws_sdk_lambda::types::Runtime::Python312)
        .role("arn:aws:iam::123456789012:role/test-role")
        .handler("index.handler")
        .code(
            aws_sdk_lambda::types::FunctionCode::builder()
                .zip_file(Blob::new(make_python_zip()))
                .build(),
        )
        .send()
        .await
        .unwrap();

    drop(client);
    server.restart().await;

    // Function survived.
    let client = server.lambda_client().await;
    let func = client
        .get_function()
        .function_name("invoke-func")
        .send()
        .await
        .unwrap();
    assert_eq!(
        func.configuration().unwrap().function_name().unwrap(),
        "invoke-func"
    );

    // Invocations buffer reset to empty.
    let resp = reqwest::get(format!(
        "{}/_fakecloud/lambda/invocations",
        server.endpoint()
    ))
    .await
    .unwrap()
    .json::<serde_json::Value>()
    .await
    .unwrap();
    let invocations = resp
        .get("invocations")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        invocations.is_empty(),
        "invocations buffer should reset on restart, got: {resp:?}"
    );
}

/// Deletion survives a restart.
#[tokio::test]
async fn persistence_deletion_survives_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let client = server.lambda_client().await;

    client
        .create_function()
        .function_name("doomed-func")
        .runtime(aws_sdk_lambda::types::Runtime::Python312)
        .role("arn:aws:iam::123456789012:role/test-role")
        .handler("index.handler")
        .code(
            aws_sdk_lambda::types::FunctionCode::builder()
                .zip_file(Blob::new(make_python_zip()))
                .build(),
        )
        .send()
        .await
        .unwrap();

    client
        .delete_function()
        .function_name("doomed-func")
        .send()
        .await
        .unwrap();

    drop(client);
    server.restart().await;
    let client = server.lambda_client().await;

    let listed = client.list_functions().send().await.unwrap();
    assert!(!listed
        .functions()
        .iter()
        .any(|f| f.function_name() == Some("doomed-func")));
}
