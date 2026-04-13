use clap::Parser;

#[derive(Parser)]
#[command(name = "fakecloud")]
#[command(about = "FakeCloud — local AWS cloud emulator")]
#[command(version)]
pub(crate) struct Cli {
    /// Listen address
    #[arg(long, default_value = "0.0.0.0:4566", env = "FAKECLOUD_ADDR")]
    pub addr: String,

    /// AWS region to advertise
    #[arg(long, default_value = "us-east-1", env = "FAKECLOUD_REGION")]
    pub region: String,

    /// AWS account ID to use
    #[arg(long, default_value = "123456789012", env = "FAKECLOUD_ACCOUNT_ID")]
    pub account_id: String,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info", env = "FAKECLOUD_LOG")]
    pub log_level: String,
}

impl Cli {
    /// Derive the public-facing endpoint URL from the configured bind address.
    /// Wildcard hosts (``0.0.0.0`` / ``[::]``) are rewritten to ``localhost`` so
    /// the URL is meaningful when handed back to clients.
    pub fn endpoint_url(&self) -> String {
        let addr = &self.addr;
        let port = addr.rsplit(':').next().unwrap_or("4566");
        let host = addr.rsplit_once(':').map(|(h, _)| h).unwrap_or("0.0.0.0");
        let host = if host == "0.0.0.0" || host == "[::]" {
            "localhost"
        } else {
            host
        };
        format!("http://{host}:{port}")
    }
}
