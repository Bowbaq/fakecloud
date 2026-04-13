/// Emit a one-shot warning that a given service does not yet honor
/// `PersistenceConfig` and will continue to run purely in memory.
pub fn warn_unsupported(service_name: &str) {
    tracing::warn!(
        service = service_name,
        "persistence not yet supported, running in-memory"
    );
}
