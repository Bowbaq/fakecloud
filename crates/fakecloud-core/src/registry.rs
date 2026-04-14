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
