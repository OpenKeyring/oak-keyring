use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

#[cfg(unix)]
struct OkProcess {
    child: Option<Child>,
}

#[cfg(unix)]
impl OkProcess {
    fn spawn(vault_dir: &std::path::Path) -> Self {
        Self {
            child: Some(spawn_ok_with_pty(vault_dir)),
        }
    }

    fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        self.child
            .as_mut()
            .expect("child should be present")
            .try_wait()
    }

    fn terminate(&mut self) {
        if let Some(mut child) = self.child.take() {
            if child.try_wait().ok().flatten().is_none() {
                send_sigterm(&child);
            }
            let _ = child.wait();
        }
    }
}

#[cfg(unix)]
impl Drop for OkProcess {
    fn drop(&mut self) {
        self.terminate();
    }
}

/// Allocate a PTY for the subprocess using the `script` command.
/// This allows the TUI to initialize its terminal (raw mode, alternate screen).
/// Returns the process group ID for use in signal handling.
#[cfg(unix)]
fn spawn_ok_with_pty(vault_dir: &std::path::Path) -> Child {
    let ok_bin = env!("CARGO_BIN_EXE_ok");

    // `script -q /dev/null` allocates a PTY and runs the command
    // On macOS: script -q /dev/null /path/to/ok
    // On Linux: script -qec "/path/to/ok" /dev/null
    let mut cmd = if cfg!(target_os = "macos") {
        let mut c = Command::new("script");
        c.args(["-q", "/dev/null", ok_bin]);
        c
    } else {
        let mut c = Command::new("script");
        c.args(["-qe", "-c", ok_bin, "/dev/null"]);
        c
    };

    // Create a new process group so we can signal the entire group
    // SAFETY: pre_exec runs between fork and exec — setsid() is safe here
    // as the child process has no controlling terminal yet.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }

    cmd.env("OAK_VAULT_DIR", vault_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("should spawn ok with PTY")
}

/// Send SIGTERM to gracefully shut down the TUI process group.
#[cfg(unix)]
fn send_sigterm(child: &Child) {
    // Send SIGTERM to the process group
    // SAFETY: child.id() returns a valid PID; getpgid queries the kernel
    // for the process group ID of that PID — no pointer dereference involved.
    let pgid = unsafe { libc::getpgid(child.id() as i32) };
    if pgid > 0 {
        // SAFETY: pgid is verified positive above. kill(-pgid, sig) sends SIGTERM
        // to all processes in process group pgid, which is safe per POSIX.
        unsafe {
            libc::kill(-pgid, libc::SIGTERM);
        }
    }
}

/// Verify that a second `ok` process is blocked when the first holds the lock.
#[test]
#[cfg(unix)]
fn second_process_is_blocked_when_lock_is_held() {
    let dir = tempfile::tempdir().unwrap();
    let vault_dir = dir.path().to_path_buf();

    let _first = OkProcess::spawn(&vault_dir);

    // Wait for the first instance to start and acquire the lock
    thread::sleep(Duration::from_secs(2));

    // Try second instance — should fail with "already running"
    let second = Command::new(env!("CARGO_BIN_EXE_ok"))
        .env("OAK_VAULT_DIR", &vault_dir)
        .output()
        .expect("ok binary should run");

    let stderr = String::from_utf8_lossy(&second.stderr);
    assert!(
        !second.status.success(),
        "second instance should exit with failure"
    );
    assert!(
        stderr.contains("already running"),
        "stderr should contain 'already running', got: {stderr}"
    );

    // `_first` terminates the subprocess on drop, including panic paths.
}

/// Verify that after the first instance exits, a new one can acquire the lock.
#[test]
#[cfg(unix)]
fn lock_released_after_first_instance_exits() {
    let dir = tempfile::tempdir().unwrap();
    let vault_dir = dir.path().to_path_buf();

    // Start and then stop first instance
    let mut first = OkProcess::spawn(&vault_dir);
    thread::sleep(Duration::from_secs(2));
    first.terminate();
    thread::sleep(Duration::from_millis(500));

    // Second instance should be able to start (not blocked by stale lock)
    let mut second = OkProcess::spawn(&vault_dir);
    thread::sleep(Duration::from_secs(1));

    // Verify second instance is running (not blocked)
    let status = second.try_wait().expect("should be able to check status");
    assert!(
        status.is_none(),
        "second instance should still be running (lock was released)"
    );
}
