//! Shared harness for spawning a local fakecloud process under test.
//!
//! Consumed by `fakecloud-e2e`, `fakecloud-parity`, and any other test crate
//! that wants a real fakecloud binary on a random port without each crate
//! rolling its own process-lifecycle code.
//!
//! Scope is intentionally narrow: spawn / endpoint / `SdkConfig` / cleanup.
//! Per-service SDK client factories live in each consumer so this crate
//! doesn't drag in every `aws-sdk-*` dependency.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::Duration;

use aws_config::BehaviorVersion;
use aws_credential_types::Credentials;
use aws_types::region::Region;

/// A test server that spawns fakecloud on a random port.
pub struct TestServer {
    child: Option<Child>,
    port: u16,
    endpoint: String,
    container_cli: String,
    extra_args: Vec<String>,
    env_vars: Vec<(String, String)>,
    log_level: String,
}

impl TestServer {
    /// Start a new fakecloud server on a random available port.
    pub async fn start() -> Self {
        Self::start_with_env(&[]).await
    }

    /// Start with extra environment variables passed to the server process.
    pub async fn start_with_env(env: &[(&str, &str)]) -> Self {
        Self::start_full(env, &[]).await
    }

    /// Start fakecloud in persistent mode with the given data directory.
    pub async fn start_persistent(data_path: &Path) -> Self {
        Self::start_persistent_with_cache(data_path, None).await
    }

    pub async fn start_persistent_with_cache(data_path: &Path, s3_cache_size: Option<u64>) -> Self {
        let data_path_str = data_path.display().to_string();
        let mut args: Vec<String> = vec![
            "--storage-mode".to_string(),
            "persistent".to_string(),
            "--data-path".to_string(),
            data_path_str,
        ];
        if let Some(size) = s3_cache_size {
            args.push("--s3-cache-size".to_string());
            args.push(size.to_string());
        }
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        Self::start_full(&[("FAKECLOUD_CONTAINER_CLI", "false")], &arg_refs).await
    }

    /// Full form: extra env vars + extra CLI args.
    pub async fn start_full(env: &[(&str, &str)], extra_args: &[&str]) -> Self {
        let bin = find_binary();

        let container_cli = env
            .iter()
            .find(|(k, _)| *k == "FAKECLOUD_CONTAINER_CLI")
            .map(|(_, v)| v.to_string())
            .unwrap_or_else(detect_container_cli);

        let log_level = env
            .iter()
            .find(|(k, _)| *k == "FAKECLOUD_TEST_LOG_LEVEL")
            .map(|(_, v)| v.to_string())
            .or_else(|| std::env::var("FAKECLOUD_TEST_LOG_LEVEL").ok())
            .unwrap_or_else(|| "warn".to_string());

        let env_vars: Vec<(String, String)> = env
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        let extra_args_owned: Vec<String> = extra_args.iter().map(|s| (*s).to_string()).collect();

        for _ in 0..3 {
            let port = find_available_port();
            let endpoint = format!("http://127.0.0.1:{port}");

            let mut cmd = Command::new(&bin);
            cmd.arg("--addr")
                .arg(format!("0.0.0.0:{port}"))
                .arg("--log-level")
                .arg(&log_level)
                .args(&extra_args_owned)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            for (key, value) in &env_vars {
                cmd.env(key, value);
            }

            let mut child = cmd.spawn().expect("failed to start fakecloud");

            if wait_for_port(&mut child, port).await {
                return Self {
                    child: Some(child),
                    port,
                    endpoint,
                    container_cli,
                    extra_args: extra_args_owned,
                    env_vars,
                    log_level,
                };
            }

            let _ = child.kill();
            let _ = child.wait();
        }

        panic!("fakecloud failed to start after 3 attempts");
    }

    /// Kill the current child and respawn with the same extra args/env.
    /// Allocates a new port because the previous one may still be in TIME_WAIT.
    pub async fn restart(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let bin = find_binary();
        for _ in 0..5 {
            let port = find_available_port();
            let endpoint = format!("http://127.0.0.1:{port}");
            let mut cmd = Command::new(&bin);
            cmd.arg("--addr")
                .arg(format!("0.0.0.0:{port}"))
                .arg("--log-level")
                .arg(&self.log_level)
                .args(&self.extra_args)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            for (key, value) in &self.env_vars {
                cmd.env(key, value);
            }
            let mut child = cmd.spawn().expect("failed to respawn fakecloud");
            if wait_for_port(&mut child, port).await {
                self.child = Some(child);
                self.port = port;
                self.endpoint = endpoint;
                return;
            }
            let _ = child.kill();
            let _ = child.wait();
        }
        panic!("fakecloud failed to restart");
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Create a shared AWS SDK config pointing at this test server.
    pub async fn aws_config(&self) -> aws_config::SdkConfig {
        aws_config::defaults(BehaviorVersion::latest())
            .endpoint_url(self.endpoint())
            .region(Region::new("us-east-1"))
            .credentials_provider(Credentials::new(
                "AKIAIOSFODNN7EXAMPLE",
                "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
                None,
                None,
                "test",
            ))
            .load()
            .await
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let pid = child.id();
            let _ = child.kill();
            let _ = child.wait();

            // Clean up any Lambda containers spawned by this server instance.
            let label = format!("fakecloud-instance=fakecloud-{}", pid);
            let cli = &self.container_cli;
            let output = Command::new(cli)
                .args(["ps", "-aq", "--filter", &format!("label={}", label)])
                .output();
            if let Ok(output) = output {
                let ids = String::from_utf8_lossy(&output.stdout);
                for id in ids.split_whitespace() {
                    let _ = Command::new(cli)
                        .args(["rm", "-f", id])
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status();
                }
            }
        }
    }
}

/// Run fakecloud once and collect its exit status + stderr output.
/// For tests that deliberately cause the server to fail at boot.
pub fn run_until_exit(
    extra_args: &[&str],
    env: &[(&str, &str)],
    timeout: Duration,
) -> (std::process::ExitStatus, String) {
    let bin = find_binary();
    let port = find_available_port();
    let mut cmd = Command::new(&bin);
    cmd.arg("--addr")
        .arg(format!("127.0.0.1:{port}"))
        .arg("--log-level")
        .arg("warn")
        .args(extra_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().expect("failed to spawn fakecloud");
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().expect("try_wait") {
            let output = child.wait_with_output().expect("wait_with_output");
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return (status, stderr);
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let output = child.wait_with_output().expect("wait_with_output");
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return (output.status, stderr);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

pub fn data_path_for(dir: &tempfile::TempDir) -> PathBuf {
    dir.path().to_path_buf()
}

/// Generate a unique, prefixed resource name for parity-style tests.
///
/// Every name starts with `fcparity-` so a sweep tool can safely reap
/// leftovers by prefix if a test crashes before cleanup runs.
pub fn unique_name(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("fcparity-{prefix}-{ts}-{seq:06}")
}

/// Output from an `aws` CLI invocation. Kept here so consumers that want to
/// run `aws ...` against the test server don't have to reimplement it.
pub struct CliOutput(pub Output);

impl CliOutput {
    pub fn success(&self) -> bool {
        self.0.status.success()
    }

    pub fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.0.stdout).to_string()
    }

    pub fn stderr_text(&self) -> String {
        String::from_utf8_lossy(&self.0.stderr).to_string()
    }

    pub fn stdout_json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.0.stdout).unwrap_or(serde_json::Value::Null)
    }
}

fn find_available_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind to random port");
    listener.local_addr().unwrap().port()
}

fn find_binary() -> String {
    // testkit lives at crates/fakecloud-testkit, and every consumer crate
    // also lives at crates/<name>, so ../../target is the workspace target.
    let debug_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/debug/fakecloud");
    let release_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../target/release/fakecloud"
    );

    if std::path::Path::new(debug_path).exists() {
        return debug_path.to_string();
    }
    if std::path::Path::new(release_path).exists() {
        return release_path.to_string();
    }

    panic!(
        "fakecloud binary not found. Run `cargo build --bin fakecloud` first.\n\
         Looked in:\n  {debug_path}\n  {release_path}"
    );
}

fn detect_container_cli() -> String {
    if cli_available("docker") {
        "docker".to_string()
    } else if cli_available("podman") {
        "podman".to_string()
    } else {
        "docker".to_string()
    }
}

fn cli_available(cli: &str) -> bool {
    Command::new(cli)
        .arg("info")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

async fn wait_for_port(child: &mut Child, port: u16) -> bool {
    // Two-stage readiness: (1) TCP connect succeeds, (2) an HTTP request
    // actually reaches an axum handler. A bare TCP connect only proves
    // the kernel accepted SYNs into fakecloud's listen queue -- it does
    // not prove axum has reached `serve().await` and installed request
    // handlers. Tests that hit the server immediately after a bare TCP
    // connect occasionally saw ConnectionRefused / EOF mid-flight.
    let loopback = format!("127.0.0.1:{port}");
    let wildcard = format!("0.0.0.0:{port}");
    let health_url = format!("http://127.0.0.1:{port}/");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(500))
        .build()
        .expect("build reqwest client");

    for _ in 0..300 {
        if child.try_wait().ok().flatten().is_some() {
            return false;
        }
        let tcp_ok = std::net::TcpStream::connect(&loopback).is_ok()
            || std::net::TcpStream::connect(&wildcard).is_ok();
        if tcp_ok && client.get(&health_url).send().await.is_ok() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}
