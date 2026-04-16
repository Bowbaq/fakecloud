mod helpers;

use aws_sdk_apigatewayv2::types::{IntegrationType, ProtocolType};
use helpers::TestServer;

/// API + route + integration + stage survive a restart.
#[tokio::test]
async fn persistence_round_trip_api_route_integration_stage() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let apigw = server.apigatewayv2_client().await;

    let api = apigw
        .create_api()
        .name("orders-api")
        .protocol_type(ProtocolType::Http)
        .send()
        .await
        .unwrap();
    let api_id = api.api_id().unwrap().to_string();

    let integration = apigw
        .create_integration()
        .api_id(&api_id)
        .integration_type(IntegrationType::HttpProxy)
        .integration_uri("https://example.com/orders")
        .payload_format_version("2.0")
        .send()
        .await
        .unwrap();
    let integration_id = integration.integration_id().unwrap().to_string();

    apigw
        .create_route()
        .api_id(&api_id)
        .route_key("GET /orders")
        .target(format!("integrations/{integration_id}"))
        .send()
        .await
        .unwrap();

    apigw
        .create_stage()
        .api_id(&api_id)
        .stage_name("prod")
        .auto_deploy(true)
        .send()
        .await
        .unwrap();

    server.restart().await;
    let apigw = server.apigatewayv2_client().await;

    let apis = apigw.get_apis().send().await.unwrap();
    assert!(apis
        .items()
        .iter()
        .any(|a| a.api_id() == Some(api_id.as_str()) && a.name() == Some("orders-api")));

    let routes = apigw.get_routes().api_id(&api_id).send().await.unwrap();
    assert!(routes
        .items()
        .iter()
        .any(|r| r.route_key() == Some("GET /orders")));

    let integrations = apigw
        .get_integrations()
        .api_id(&api_id)
        .send()
        .await
        .unwrap();
    assert!(integrations
        .items()
        .iter()
        .any(|i| i.integration_id() == Some(integration_id.as_str())));

    let stage = apigw
        .get_stage()
        .api_id(&api_id)
        .stage_name("prod")
        .send()
        .await
        .unwrap();
    assert_eq!(stage.stage_name(), Some("prod"));
    assert_eq!(stage.auto_deploy(), Some(true));
}

/// Deleting an API removes it (and its children) from disk too.
#[tokio::test]
async fn persistence_delete_api_survives_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let apigw = server.apigatewayv2_client().await;

    let api_id = apigw
        .create_api()
        .name("ephemeral")
        .protocol_type(ProtocolType::Http)
        .send()
        .await
        .unwrap()
        .api_id()
        .unwrap()
        .to_string();
    apigw.delete_api().api_id(&api_id).send().await.unwrap();

    server.restart().await;
    let apigw = server.apigatewayv2_client().await;

    let apis = apigw.get_apis().send().await.unwrap();
    assert!(!apis
        .items()
        .iter()
        .any(|a| a.api_id() == Some(api_id.as_str())));
}
