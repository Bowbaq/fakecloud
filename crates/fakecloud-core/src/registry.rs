use crate::service::AwsService;
use std::collections::HashMap;
use std::sync::Arc;

/// Registry of AWS services available in this FakeCloud instance.
#[derive(Default)]
pub struct ServiceRegistry {
    services: HashMap<String, Arc<dyn AwsService>>,
}

impl ServiceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, service: Arc<dyn AwsService>) {
        self.services
            .insert(service.service_name().to_string(), service);
    }

    pub fn get(&self, name: &str) -> Option<&Arc<dyn AwsService>> {
        self.services.get(name)
    }

    pub fn service_names(&self) -> Vec<&str> {
        self.services.keys().map(|s| s.as_str()).collect()
    }

    /// Partition registered services into `(iam_enforceable, not
    /// enforceable)` lists, sorted alphabetically within each group. Used
    /// by main.rs to emit the startup log when IAM enforcement is on.
    pub fn iam_enforcement_split(&self) -> (Vec<&str>, Vec<&str>) {
        let mut enforced: Vec<&str> = Vec::new();
        let mut skipped: Vec<&str> = Vec::new();
        for (name, service) in &self.services {
            if service.iam_enforceable() {
                enforced.push(name.as_str());
            } else {
                skipped.push(name.as_str());
            }
        }
        enforced.sort();
        skipped.sort();
        (enforced, skipped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::{AwsRequest, AwsResponse, AwsService, AwsServiceError};
    use async_trait::async_trait;

    struct EnforcedService {
        name: &'static str,
    }

    #[async_trait]
    impl AwsService for EnforcedService {
        fn service_name(&self) -> &str {
            self.name
        }
        async fn handle(&self, _: AwsRequest) -> Result<AwsResponse, AwsServiceError> {
            unreachable!()
        }
        fn supported_actions(&self) -> &[&str] {
            &[]
        }
        fn iam_enforceable(&self) -> bool {
            true
        }
    }

    struct UnenforcedService {
        name: &'static str,
    }

    #[async_trait]
    impl AwsService for UnenforcedService {
        fn service_name(&self) -> &str {
            self.name
        }
        async fn handle(&self, _: AwsRequest) -> Result<AwsResponse, AwsServiceError> {
            unreachable!()
        }
        fn supported_actions(&self) -> &[&str] {
            &[]
        }
    }

    #[test]
    fn new_is_empty() {
        let r = ServiceRegistry::new();
        assert!(r.service_names().is_empty());
        assert!(r.get("anything").is_none());
    }

    #[test]
    fn register_then_get_roundtrip() {
        let mut r = ServiceRegistry::new();
        r.register(Arc::new(EnforcedService { name: "s3" }));
        assert!(r.get("s3").is_some());
        assert!(r.get("missing").is_none());
    }

    #[test]
    fn service_names_collects_registered() {
        let mut r = ServiceRegistry::new();
        r.register(Arc::new(EnforcedService { name: "s3" }));
        r.register(Arc::new(UnenforcedService { name: "sts" }));
        let mut names = r.service_names();
        names.sort();
        assert_eq!(names, vec!["s3", "sts"]);
    }

    #[test]
    fn iam_split_sorts_and_separates_groups() {
        let mut r = ServiceRegistry::new();
        r.register(Arc::new(EnforcedService { name: "s3" }));
        r.register(Arc::new(EnforcedService { name: "iam" }));
        r.register(Arc::new(UnenforcedService { name: "sts" }));
        r.register(Arc::new(UnenforcedService { name: "bedrock" }));
        let (enforced, skipped) = r.iam_enforcement_split();
        assert_eq!(enforced, vec!["iam", "s3"]);
        assert_eq!(skipped, vec!["bedrock", "sts"]);
    }

    #[test]
    fn register_overwrites_same_name() {
        let mut r = ServiceRegistry::new();
        r.register(Arc::new(EnforcedService { name: "s3" }));
        r.register(Arc::new(UnenforcedService { name: "s3" }));
        let (enforced, skipped) = r.iam_enforcement_split();
        assert!(enforced.is_empty());
        assert_eq!(skipped, vec!["s3"]);
    }
}
