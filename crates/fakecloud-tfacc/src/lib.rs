//! Upstream Terraform Provider acceptance-test harness.
//!
//! Each `#[tokio::test]` in `tests/acc.rs` spawns a fakecloud process, sets
//! `TF_ACC=1` plus `AWS_ENDPOINT_URL_<SERVICE>=…` env vars, and invokes a
//! single `TestAcc*` function from `hashicorp/terraform-provider-aws` via
//! `go test`. The upstream test does its own Terraform apply/plan/destroy
//! cycle and asserts on the returned resource state — giving us semantic
//! coverage (waiters, field presence, drift) that SDK-based tests miss.
//!
//! Prior art: `bblommers/localstack-terraform-test`. We invert the model to
//! an *allow*-list rather than a deny-list to match fakecloud's
//! parity-per-implemented-service invariant.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::Duration;

pub mod allowlist;

pub use allowlist::{Service, SERVICES};

/// Pinned upstream provider tag. Bumping is a deliberate edit; newer tags
/// may add acc tests that assume fields fakecloud does not yet return.
pub const PROVIDER_TAG: &str = "v5.97.0";
pub const PROVIDER_REPO: &str = "https://github.com/hashicorp/terraform-provider-aws.git";

/// Returns true if both `go` and `terraform` are on PATH. If either is
/// missing, acc tests skip (they don't fail) — matching the
/// `FAKECLOUD_CONTAINER_CLI=false` skip pattern in fakecloud-e2e.
pub fn toolchain_available() -> bool {
    Command::new("go")
        .arg("version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
        && Command::new("terraform")
            .arg("version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
}

/// Idempotently clone + patch the upstream provider into `target/tfacc/`.
///
/// Returns the absolute path to the provider source tree, ready for
/// `go test ./internal/service/<svc>/`.
///
/// On Go ≥ 1.24 the upstream `go.mod` needs its `godebug tlskyber=0`
/// directive stripped (the pragma was removed in 1.24). We apply the strip
/// unconditionally — it's harmless on 1.23.
pub fn setup_provider_source() -> std::io::Result<PathBuf> {
    let target = provider_dir();
    if !target.exists() {
        std::fs::create_dir_all(target.parent().unwrap())?;
        let status = Command::new("git")
            .args([
                "clone",
                "--depth",
                "1",
                "--branch",
                PROVIDER_TAG,
                PROVIDER_REPO,
                &target.display().to_string(),
            ])
            .status()?;
        if !status.success() {
            return Err(std::io::Error::other(format!(
                "failed to clone {PROVIDER_REPO}@{PROVIDER_TAG}"
            )));
        }
    }
    strip_godebug(&target.join("go.mod"))?;
    Ok(target)
}

fn provider_dir() -> PathBuf {
    // target/tfacc/terraform-provider-aws — sibling to target/debug
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .join("..")
        .join("..")
        .join("target")
        .join("tfacc")
        .join(format!("terraform-provider-aws-{PROVIDER_TAG}"))
}

fn strip_godebug(go_mod: &Path) -> std::io::Result<()> {
    let contents = std::fs::read_to_string(go_mod)?;
    if !contents.contains("godebug tlskyber") {
        return Ok(());
    }
    // Preserve the original line endings (including whether the file has a
    // trailing newline) by splitting on `\n` rather than `lines()`, which
    // silently swallows the final empty element.
    let stripped: String = contents
        .split_inclusive('\n')
        .filter(|line| !line.trim_start().starts_with("godebug tlskyber"))
        .collect();
    std::fs::write(go_mod, stripped)
}

/// Minimal fakecloud test server — lifecycle only, no SDK clients.
pub struct TestServer {
    child: Option<Child>,
    port: u16,
}

impl TestServer {
    pub async fn start() -> Self {
        let bin = find_binary();
        for _ in 0..3 {
            let port = find_available_port();
            let mut child = Command::new(&bin)
                .arg("--addr")
                .arg(format!("0.0.0.0:{port}"))
                .arg("--log-level")
                .arg("warn")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("failed to start fakecloud");
            if wait_for_ready(&mut child, port).await {
                return Self {
                    child: Some(child),
                    port,
                };
            }
            let _ = child.kill();
            let _ = child.wait();
        }
        panic!("fakecloud failed to start after 3 attempts");
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn endpoint(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Runs every `TestAcc*` test for a service — minus deny-listed names —
/// against a running fakecloud instance.
///
/// We intentionally run the whole service at once rather than one Go test
/// per Rust test: provider process startup dominates per-test time, and
/// `go test -skip` lets us exclude unsupportable tests cheaply.
pub struct GoTestRunner<'a> {
    pub provider_root: &'a Path,
    pub endpoint: String,
}

impl<'a> GoTestRunner<'a> {
    pub fn run_service(&self, service: &Service) -> GoTestResult {
        let service_path = format!("./internal/service/{}/", service.name);
        let run_re = service.run_regex;
        let skip_re = if service.deny.is_empty() {
            String::new()
        } else {
            format!("^({})$", service.deny.join("|"))
        };

        // `-parallel 8` lets Go's test runner execute up to 8 `t.Parallel()`
        // subtests concurrently within a single `go test` invocation. Most
        // upstream TestAcc* functions opt into parallelism, so this is the
        // main lever for wall-time inside a single service. CI fan-out
        // across services is handled by the GitHub Actions matrix.
        let mut cmd = Command::new("go");
        let mut args: Vec<String> = vec![
            "test".into(),
            service_path,
            "-run".into(),
            run_re.into(),
            "-v".into(),
            "-timeout".into(),
            "60m".into(),
            "-count=1".into(),
            "-parallel".into(),
            "8".into(),
        ];
        if !skip_re.is_empty() {
            args.push("-skip".into());
            args.push(skip_re);
        }
        cmd.args(&args)
            .current_dir(self.provider_root)
            .env("TF_ACC", "1")
            .env("AWS_ACCESS_KEY_ID", "test")
            .env("AWS_SECRET_ACCESS_KEY", "test")
            .env("AWS_DEFAULT_REGION", "us-east-1")
            .env("AWS_REGION", "us-east-1")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Route every service we care about to the single fakecloud endpoint.
        // AWS SDK Go v2 honours AWS_ENDPOINT_URL_<SERVICE>; these override any
        // default endpoint lookup.
        for (key, _service_id) in ENDPOINT_ENV_VARS {
            cmd.env(key, &self.endpoint);
        }

        let output = cmd.output().expect("run go test");
        GoTestResult {
            success: output.status.success(),
            output,
        }
    }
}

pub struct GoTestResult {
    pub success: bool,
    pub output: Output,
}

impl GoTestResult {
    /// Panics with the last few lines of `go test` output when the service
    /// run had any failing upstream test. Keeps passing runs silent.
    pub fn assert_pass(self, service: &str) {
        if self.success {
            return;
        }
        let stdout = String::from_utf8_lossy(&self.output.stdout);
        let stderr = String::from_utf8_lossy(&self.output.stderr);
        let fails: Vec<&str> = stdout.lines().filter(|l| l.contains("--- FAIL:")).collect();
        let combined = format!("--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}");
        let lines: Vec<&str> = combined.lines().collect();
        let start = lines.len().saturating_sub(200);
        let tail = lines[start..].join("\n");
        panic!(
            "upstream TestAcc failures in service `{service}` ({} failed):\n{}\n\n{tail}",
            fails.len(),
            fails.join("\n")
        );
    }
}

/// `(env_var, provider_service_id)` — set every entry to the fakecloud
/// endpoint so a single acc test can touch multiple services.
pub const ENDPOINT_ENV_VARS: &[(&str, &str)] = &[
    ("AWS_ENDPOINT_URL", "default"),
    ("AWS_ENDPOINT_URL_SQS", "sqs"),
    ("AWS_ENDPOINT_URL_SNS", "sns"),
    ("AWS_ENDPOINT_URL_S3", "s3"),
    ("AWS_ENDPOINT_URL_IAM", "iam"),
    ("AWS_ENDPOINT_URL_STS", "sts"),
    ("AWS_ENDPOINT_URL_SSM", "ssm"),
    ("AWS_ENDPOINT_URL_DYNAMODB", "dynamodb"),
    ("AWS_ENDPOINT_URL_LAMBDA", "lambda"),
    ("AWS_ENDPOINT_URL_SECRETSMANAGER", "secretsmanager"),
    ("AWS_ENDPOINT_URL_EVENTBRIDGE", "eventbridge"),
    ("AWS_ENDPOINT_URL_KMS", "kms"),
    ("AWS_ENDPOINT_URL_LOGS", "logs"),
    ("AWS_ENDPOINT_URL_KINESIS", "kinesis"),
    ("AWS_ENDPOINT_URL_RDS", "rds"),
    ("AWS_ENDPOINT_URL_ELASTICACHE", "elasticache"),
    ("AWS_ENDPOINT_URL_CLOUDFORMATION", "cloudformation"),
    ("AWS_ENDPOINT_URL_SESV2", "sesv2"),
    ("AWS_ENDPOINT_URL_SES", "ses"),
    ("AWS_ENDPOINT_URL_COGNITO_IDP", "cognitoidp"),
    ("AWS_ENDPOINT_URL_SFN", "sfn"),
    ("AWS_ENDPOINT_URL_APIGATEWAYV2", "apigatewayv2"),
    ("AWS_ENDPOINT_URL_BEDROCK", "bedrock"),
];

fn find_available_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind to random port");
    listener.local_addr().unwrap().port()
}

fn find_binary() -> PathBuf {
    let candidates = [
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/debug/fakecloud"),
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../target/release/fakecloud"
        ),
    ];
    for path in candidates {
        if Path::new(path).exists() {
            return PathBuf::from(path);
        }
    }
    panic!("fakecloud binary not found. Run `cargo build --bin fakecloud` first.");
}

async fn wait_for_ready(child: &mut Child, port: u16) -> bool {
    let health = format!("http://127.0.0.1:{port}/");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(500))
        .build()
        .expect("build reqwest client");
    for _ in 0..300 {
        if child.try_wait().ok().flatten().is_some() {
            return false;
        }
        if client.get(&health).send().await.is_ok() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}
