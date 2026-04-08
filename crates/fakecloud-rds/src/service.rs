use async_trait::async_trait;
use http::StatusCode;

use fakecloud_aws::xml::xml_escape;
use fakecloud_core::service::{AwsRequest, AwsResponse, AwsService, AwsServiceError};

use crate::state::{
    default_engine_versions, default_orderable_options, EngineVersionInfo,
    OrderableDbInstanceOption, SharedRdsState,
};

const RDS_NS: &str = "http://rds.amazonaws.com/doc/2014-10-31/";
const SUPPORTED_ACTIONS: &[&str] = &[
    "DescribeDBEngineVersions",
    "DescribeOrderableDBInstanceOptions",
];

pub struct RdsService {
    state: SharedRdsState,
}

impl RdsService {
    pub fn new(state: SharedRdsState) -> Self {
        Self { state }
    }
}

#[async_trait]
impl AwsService for RdsService {
    fn service_name(&self) -> &str {
        "rds"
    }

    async fn handle(&self, request: AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let _ = &self.state;
        match request.action.as_str() {
            "DescribeDBEngineVersions" => self.describe_db_engine_versions(&request),
            "DescribeOrderableDBInstanceOptions" => {
                self.describe_orderable_db_instance_options(&request)
            }
            _ => Err(AwsServiceError::action_not_implemented(
                self.service_name(),
                &request.action,
            )),
        }
    }

    fn supported_actions(&self) -> &[&str] {
        SUPPORTED_ACTIONS
    }
}

impl RdsService {
    fn describe_db_engine_versions(
        &self,
        request: &AwsRequest,
    ) -> Result<AwsResponse, AwsServiceError> {
        let engine = optional_param(request, "Engine");
        let engine_version = optional_param(request, "EngineVersion");
        let family = optional_param(request, "DBParameterGroupFamily");
        let default_only = parse_optional_bool(optional_param(request, "DefaultOnly").as_deref())?;

        let mut versions = filter_engine_versions(
            &default_engine_versions(),
            &engine,
            &engine_version,
            &family,
        );

        if default_only.unwrap_or(false) {
            versions.truncate(1);
        }

        Ok(AwsResponse::xml(
            StatusCode::OK,
            xml_wrap(
                "DescribeDBEngineVersions",
                &format!(
                    "<DBEngineVersions>{}</DBEngineVersions>",
                    versions.iter().map(engine_version_xml).collect::<String>()
                ),
                &request.request_id,
            ),
        ))
    }

    fn describe_orderable_db_instance_options(
        &self,
        request: &AwsRequest,
    ) -> Result<AwsResponse, AwsServiceError> {
        let engine = optional_param(request, "Engine");
        let engine_version = optional_param(request, "EngineVersion");
        let db_instance_class = optional_param(request, "DBInstanceClass");
        let license_model = optional_param(request, "LicenseModel");
        let vpc = parse_optional_bool(optional_param(request, "Vpc").as_deref())?;

        let options = filter_orderable_options(
            &default_orderable_options(),
            &engine,
            &engine_version,
            &db_instance_class,
            &license_model,
            vpc,
        );

        Ok(AwsResponse::xml(
            StatusCode::OK,
            xml_wrap(
                "DescribeOrderableDBInstanceOptions",
                &format!(
                    "<OrderableDBInstanceOptions>{}</OrderableDBInstanceOptions>",
                    options.iter().map(orderable_option_xml).collect::<String>()
                ),
                &request.request_id,
            ),
        ))
    }
}

fn optional_param(req: &AwsRequest, name: &str) -> Option<String> {
    req.query_params
        .get(name)
        .cloned()
        .filter(|value| !value.is_empty())
}

fn parse_optional_bool(value: Option<&str>) -> Result<Option<bool>, AwsServiceError> {
    value
        .map(|raw| match raw {
            "true" | "True" | "TRUE" => Ok(true),
            "false" | "False" | "FALSE" => Ok(false),
            _ => Err(AwsServiceError::aws_error(
                StatusCode::BAD_REQUEST,
                "InvalidParameterValue",
                format!("Boolean parameter value '{raw}' is invalid."),
            )),
        })
        .transpose()
}

fn filter_engine_versions(
    versions: &[EngineVersionInfo],
    engine: &Option<String>,
    engine_version: &Option<String>,
    family: &Option<String>,
) -> Vec<EngineVersionInfo> {
    versions
        .iter()
        .filter(|candidate| {
            engine
                .as_ref()
                .is_none_or(|expected| candidate.engine == *expected)
        })
        .filter(|candidate| {
            engine_version
                .as_ref()
                .is_none_or(|expected| candidate.engine_version == *expected)
        })
        .filter(|candidate| {
            family
                .as_ref()
                .is_none_or(|expected| candidate.db_parameter_group_family == *expected)
        })
        .cloned()
        .collect()
}

fn filter_orderable_options(
    options: &[OrderableDbInstanceOption],
    engine: &Option<String>,
    engine_version: &Option<String>,
    db_instance_class: &Option<String>,
    license_model: &Option<String>,
    vpc: Option<bool>,
) -> Vec<OrderableDbInstanceOption> {
    options
        .iter()
        .filter(|candidate| {
            engine
                .as_ref()
                .is_none_or(|expected| candidate.engine == *expected)
        })
        .filter(|candidate| {
            engine_version
                .as_ref()
                .is_none_or(|expected| candidate.engine_version == *expected)
        })
        .filter(|candidate| {
            db_instance_class
                .as_ref()
                .is_none_or(|expected| candidate.db_instance_class == *expected)
        })
        .filter(|candidate| {
            license_model
                .as_ref()
                .is_none_or(|expected| candidate.license_model == *expected)
        })
        .filter(|_| vpc.unwrap_or(true))
        .cloned()
        .collect()
}

fn xml_wrap(action: &str, inner: &str, request_id: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
         <{action}Response xmlns=\"{RDS_NS}\">\
         <{action}Result>{inner}</{action}Result>\
         <ResponseMetadata><RequestId>{request_id}</RequestId></ResponseMetadata>\
         </{action}Response>"
    )
}

fn engine_version_xml(version: &EngineVersionInfo) -> String {
    format!(
        "<DBEngineVersion>\
         <Engine>{}</Engine>\
         <EngineVersion>{}</EngineVersion>\
         <DBParameterGroupFamily>{}</DBParameterGroupFamily>\
         <DBEngineDescription>{}</DBEngineDescription>\
         <DBEngineVersionDescription>{}</DBEngineVersionDescription>\
         <Status>{}</Status>\
         </DBEngineVersion>",
        xml_escape(&version.engine),
        xml_escape(&version.engine_version),
        xml_escape(&version.db_parameter_group_family),
        xml_escape(&version.db_engine_description),
        xml_escape(&version.db_engine_version_description),
        xml_escape(&version.status),
    )
}

fn orderable_option_xml(option: &OrderableDbInstanceOption) -> String {
    format!(
        "<OrderableDBInstanceOption>\
         <Engine>{}</Engine>\
         <EngineVersion>{}</EngineVersion>\
         <DBInstanceClass>{}</DBInstanceClass>\
         <LicenseModel>{}</LicenseModel>\
         <AvailabilityZones><AvailabilityZone><Name>us-east-1a</Name></AvailabilityZone></AvailabilityZones>\
         <MultiAZCapable>true</MultiAZCapable>\
         <ReadReplicaCapable>true</ReadReplicaCapable>\
         <Vpc>true</Vpc>\
         <SupportsStorageEncryption>true</SupportsStorageEncryption>\
         <StorageType>{}</StorageType>\
         <SupportsIops>false</SupportsIops>\
         <MinStorageSize>{}</MinStorageSize>\
         <MaxStorageSize>{}</MaxStorageSize>\
         <SupportsIAMDatabaseAuthentication>true</SupportsIAMDatabaseAuthentication>\
         </OrderableDBInstanceOption>",
        xml_escape(&option.engine),
        xml_escape(&option.engine_version),
        xml_escape(&option.db_instance_class),
        xml_escape(&option.license_model),
        xml_escape(&option.storage_type),
        option.min_storage_size,
        option.max_storage_size,
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use bytes::Bytes;
    use http::{HeaderMap, Method};

    use super::{filter_engine_versions, filter_orderable_options, RdsService};
    use crate::state::{default_engine_versions, default_orderable_options, RdsState};
    use fakecloud_core::service::{AwsRequest, AwsService};
    use parking_lot::RwLock;
    use std::sync::Arc;

    #[test]
    fn filter_engine_versions_matches_requested_engine() {
        let versions = default_engine_versions();

        let filtered =
            filter_engine_versions(&versions, &Some("postgres".to_string()), &None, &None);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].engine, "postgres");
    }

    #[test]
    fn filter_orderable_options_respects_instance_class() {
        let options = default_orderable_options();

        let filtered = filter_orderable_options(
            &options,
            &Some("postgres".to_string()),
            &Some("16.3".to_string()),
            &Some("db.t3.micro".to_string()),
            &None,
            Some(true),
        );

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].db_instance_class, "db.t3.micro");
    }

    #[tokio::test]
    async fn describe_engine_versions_returns_xml_body() {
        let service = RdsService::new(Arc::new(RwLock::new(RdsState::new(
            "123456789012",
            "us-east-1",
        ))));
        let request = request("DescribeDBEngineVersions", &[("Engine", "postgres")]);

        let response = service.handle(request).await.expect("response");
        let body = String::from_utf8(response.body.to_vec()).expect("utf8");

        assert!(body.contains("<DescribeDBEngineVersionsResponse"));
        assert!(body.contains("<Engine>postgres</Engine>"));
        assert!(body.contains("<DBParameterGroupFamily>postgres16</DBParameterGroupFamily>"));
    }

    fn request(action: &str, params: &[(&str, &str)]) -> AwsRequest {
        let mut query_params = HashMap::from([("Action".to_string(), action.to_string())]);
        for (key, value) in params {
            query_params.insert((*key).to_string(), (*value).to_string());
        }

        AwsRequest {
            service: "rds".to_string(),
            action: action.to_string(),
            region: "us-east-1".to_string(),
            account_id: "123456789012".to_string(),
            request_id: "test-request-id".to_string(),
            headers: HeaderMap::new(),
            query_params,
            body: Bytes::new(),
            path_segments: vec![],
            raw_path: "/".to_string(),
            raw_query: String::new(),
            method: Method::POST,
            is_query_protocol: true,
            access_key_id: None,
        }
    }
}
