//! Startup reaper for orphaned backing containers.
//!
//! fakecloud spawns docker containers for RDS (postgres), ElastiCache (redis),
//! and Lambda (runtime images) and labels each one with
//! `fakecloud-instance=fakecloud-<server-pid>`. Normal shutdown runs
//! `stop_all()` on each runtime, but if the server was killed with SIGKILL
//! (or crashed, or OOM'd) those containers outlive the process and pile up.
//!
//! On startup we list every container carrying the `fakecloud-instance`
//! label, parse the owning PID out of the label value, and remove any
//! container whose owner is no longer alive. Containers owned by the
//! currently-running fakecloud process are always skipped.

use std::process::{Command, Stdio};

/// Reap orphaned fakecloud-owned containers whose server PID is no longer alive.
///
/// Uses the same CLI detection policy as the runtimes: honors
/// `FAKECLOUD_CONTAINER_CLI` if set, otherwise tries `docker` then `podman`.
/// If no container CLI is available this is a silent no-op — fakecloud is
/// expected to start fine without docker.
pub fn reap_stale_containers() {
    let Some(cli) = detect_cli() else {
        return;
    };

    let output = match Command::new(&cli)
        .args([
            "ps",
            "-a",
            "--filter",
            "label=fakecloud-instance",
            "--format",
            "{{.ID}} {{.Label \"fakecloud-instance\"}}",
        ])
        .stderr(Stdio::null())
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return,
    };

    let self_pid = std::process::id();
    let listing = String::from_utf8_lossy(&output.stdout);
    let mut reaped = 0usize;

    for line in listing.lines() {
        let Some((id, label)) = line.split_once(' ') else {
            continue;
        };
        let Some(pid_str) = label.strip_prefix("fakecloud-") else {
            continue;
        };
        let Ok(pid) = pid_str.parse::<u32>() else {
            continue;
        };
        if pid == self_pid || pid_alive(pid) {
            continue;
        }
        let removed = Command::new(&cli)
            .args(["rm", "-f", id])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if removed {
            reaped += 1;
        }
    }

    if reaped > 0 {
        tracing::info!(count = reaped, "reaped orphaned backing containers");
    }
}

fn detect_cli() -> Option<String> {
    if let Ok(cli) = std::env::var("FAKECLOUD_CONTAINER_CLI") {
        return cli_works(&cli).then_some(cli);
    }
    if cli_works("docker") {
        return Some("docker".to_string());
    }
    if cli_works("podman") {
        return Some("podman".to_string());
    }
    None
}

fn cli_works(cli: &str) -> bool {
    Command::new(cli)
        .arg("info")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// True if the given PID is a live process on this host.
///
/// On Unix we use `kill(pid, 0)`: it returns 0 if the process exists
/// (including zombies), or sets `errno` to `ESRCH` if not. On non-Unix
/// platforms we conservatively return `true` so the reaper never removes
/// a container it can't prove is orphaned.
#[cfg(unix)]
pub fn pid_alive(pid: u32) -> bool {
    // SAFETY: `kill` with signal 0 is a liveness probe; it does not
    // actually deliver a signal. Any PID value is safe to pass.
    let rc = unsafe { libc::kill(pid as libc::pid_t, 0) };
    if rc == 0 {
        return true;
    }
    // errno == EPERM means the process exists but we can't signal it —
    // still alive from our perspective.
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(not(unix))]
pub fn pid_alive(_pid: u32) -> bool {
    true
}

#[cfg(all(test, unix))]
mod tests {
    use super::{cli_works, pid_alive};

    #[test]
    fn self_is_alive() {
        assert!(pid_alive(std::process::id()));
    }

    #[test]
    fn init_is_alive() {
        assert!(pid_alive(1));
    }

    #[test]
    fn huge_pid_is_dead() {
        // Max u32 is far outside any reasonable PID range on any OS.
        assert!(!pid_alive(u32::MAX - 1));
    }

    #[test]
    fn cli_works_false_for_unknown_binary() {
        assert!(!cli_works("definitely-not-a-real-cli-name-xyz123"));
    }
}
