use std::io::{ErrorKind, Read};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::fs::File;
#[cfg(unix)]
use std::os::fd::FromRawFd;
#[cfg(unix)]
use std::os::unix::process::CommandExt;

const WAIT_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(50);
const RUNNING_STABILITY_WINDOW: Duration = Duration::from_millis(300);
const TERMINATE_TIMEOUT: Duration = Duration::from_secs(2);
const LOCK_FILENAME: &str = ".instance.lock";

#[cfg(unix)]
struct OkProcess {
    child: Option<Child>,
    pty_master: File,
}

#[cfg(unix)]
impl OkProcess {
    fn spawn(vault_dir: &std::path::Path) -> Self {
        let (child, pty_master) = spawn_ok_with_pty(vault_dir);
        Self {
            child: Some(child),
            pty_master,
        }
    }

    fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        self.child
            .as_mut()
            .expect("child should be present")
            .try_wait()
    }

    fn terminate_and_wait(&mut self) -> Option<std::process::ExitStatus> {
        if let Some(mut child) = self.child.take() {
            if child.try_wait().ok().flatten().is_none() {
                send_sigterm(&child);
            }
            if let Some(status) = wait_for_child_exit(&mut child, TERMINATE_TIMEOUT) {
                return Some(status);
            }

            send_sigkill(&child);
            return child.wait().ok();
        }
        None
    }

    fn terminate(&mut self) {
        let _ = self.terminate_and_wait();
    }

    fn wait_for_tui_output(&mut self) {
        let deadline = Instant::now() + WAIT_TIMEOUT;
        let mut buf = [0_u8; 1024];

        while Instant::now() < deadline {
            if let Some(status) = self.try_wait().expect("should be able to check status") {
                panic!("process exited before rendering TUI output: {status}");
            }

            match self.pty_master.read(&mut buf) {
                Ok(n) if n > 0 => return,
                Ok(_) => {}
                Err(e) if e.kind() == ErrorKind::WouldBlock => {}
                Err(e) => panic!("failed reading TUI PTY output: {e}"),
            }

            thread::sleep(POLL_INTERVAL);
        }

        panic!("timed out waiting for TUI output");
    }
}

#[cfg(unix)]
impl Drop for OkProcess {
    fn drop(&mut self) {
        self.terminate();
    }
}

/// Allocate a PTY for the subprocess.
/// This allows the TUI to initialize its terminal (raw mode, alternate screen).
/// Returns the process group ID for use in signal handling.
#[cfg(unix)]
fn spawn_ok_with_pty(vault_dir: &std::path::Path) -> (Child, File) {
    let ok_bin = env!("CARGO_BIN_EXE_ok");
    let mut master_fd = -1;
    let mut slave_fd = -1;

    // SAFETY: openpty initializes valid master/slave file descriptors when it
    // returns 0. Null termios/winsize pointers request platform defaults.
    let openpty_result = unsafe {
        libc::openpty(
            &mut master_fd,
            &mut slave_fd,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    assert_eq!(openpty_result, 0, "openpty should allocate a PTY");
    set_close_on_exec(master_fd);
    set_nonblocking(master_fd);

    // SAFETY: openpty returned owned file descriptors above.
    let pty_master = unsafe { File::from_raw_fd(master_fd) };
    // SAFETY: openpty returned owned file descriptors above.
    let pty_slave = unsafe { File::from_raw_fd(slave_fd) };

    let mut cmd = Command::new(ok_bin);

    // Create a new process group so we can signal the entire group
    // SAFETY: pre_exec runs between fork and exec — setsid() is safe here
    // as the child process has no controlling terminal yet.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }

    cmd.env("XDG_DATA_HOME", vault_dir)
        .env("XDG_CONFIG_HOME", vault_dir)
        .stdin(Stdio::from(
            pty_slave.try_clone().expect("should clone PTY slave"),
        ))
        .stdout(Stdio::from(
            pty_slave.try_clone().expect("should clone PTY slave"),
        ))
        .stderr(Stdio::from(pty_slave))
        .spawn()
        .map(|child| (child, pty_master))
        .expect("should spawn ok with PTY")
}

#[cfg(unix)]
fn set_close_on_exec(fd: i32) {
    // SAFETY: fcntl with F_GETFD/F_SETFD reads and updates descriptor flags for
    // a valid fd returned by openpty.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    assert_ne!(flags, -1, "should read fd flags");
    let result = unsafe { libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) };
    assert_ne!(result, -1, "should set FD_CLOEXEC");
}

#[cfg(unix)]
fn set_nonblocking(fd: i32) {
    // SAFETY: fcntl with F_GETFL/F_SETFL reads and updates descriptor status
    // flags for a valid fd returned by openpty.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    assert_ne!(flags, -1, "should read fd status flags");
    let result = unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) };
    assert_ne!(result, -1, "should set O_NONBLOCK");
}

#[cfg(unix)]
fn wait_for_child_exit(child: &mut Child, timeout: Duration) -> Option<std::process::ExitStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().ok().flatten() {
            return Some(status);
        }

        if Instant::now() >= deadline {
            return None;
        }

        thread::sleep(POLL_INTERVAL);
    }
}

/// Send SIGTERM to gracefully shut down the TUI process group.
#[cfg(unix)]
fn send_sigterm(child: &Child) {
    send_signal_to_process_group(child, libc::SIGTERM);
}

#[cfg(unix)]
fn send_sigkill(child: &Child) {
    send_signal_to_process_group(child, libc::SIGKILL);
}

#[cfg(unix)]
fn send_signal_to_process_group(child: &Child, signal: i32) {
    // Send the signal to the process group.
    // SAFETY: child.id() returns a valid PID; getpgid queries the kernel
    // for the process group ID of that PID — no pointer dereference involved.
    let pgid = unsafe { libc::getpgid(child.id() as i32) };
    if pgid > 0 {
        // SAFETY: pgid is verified positive above. kill(-pgid, sig) sends signal
        // to all processes in process group pgid, which is safe per POSIX.
        unsafe {
            libc::kill(-pgid, signal);
        }
    }
}

#[cfg(unix)]
fn run_ok_once(vault_dir: &std::path::Path) -> Output {
    // Create oak-keyring subdirectories (paths::data_dir() appends "oak-keyring")
    let data_dir = vault_dir.join("oak-keyring");
    let config_dir = vault_dir.join("oak-keyring");
    std::fs::create_dir_all(&data_dir).expect("failed to create data dir");
    std::fs::create_dir_all(&config_dir).expect("failed to create config dir");

    Command::new(env!("CARGO_BIN_EXE_ok"))
        .env("XDG_DATA_HOME", vault_dir)
        .env("XDG_CONFIG_HOME", vault_dir)
        .output()
        .expect("ok binary should run")
}

#[cfg(unix)]
fn wait_for_lock_file_created_by_process(process: &mut OkProcess, vault_dir: &std::path::Path) {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    // Lock file is created in data_dir, which is XDG_DATA_HOME/oak-keyring
    let lock_path = vault_dir.join("oak-keyring").join(LOCK_FILENAME);
    let mut seen_lock_at = None;

    while Instant::now() < deadline {
        match process.try_wait().expect("should be able to check status") {
            None => {}
            Some(status) => panic!("first instance exited before it was stable: {status}"),
        }

        if lock_path.is_file() {
            let seen_at = *seen_lock_at.get_or_insert_with(Instant::now);
            if Instant::now().duration_since(seen_at) >= RUNNING_STABILITY_WINDOW {
                return;
            }
        } else {
            seen_lock_at = None;
        }

        thread::sleep(POLL_INTERVAL);
    }

    panic!("timed out waiting for first instance to create and hold the lock file");
}

#[cfg(unix)]
fn wait_for_second_instance_blocked(first: &mut OkProcess, vault_dir: &std::path::Path) -> Output {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    let mut last_stderr = None;

    while Instant::now() < deadline {
        if let Some(status) = first.try_wait().expect("should be able to check status") {
            panic!("first instance exited before second instance was blocked: {status}");
        }

        let output = run_ok_once(vault_dir);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !output.status.success() && stderr.contains("already running") {
            return output;
        }
        last_stderr = Some(stderr.into_owned());

        thread::sleep(POLL_INTERVAL);
    }

    panic!(
        "timed out waiting for second instance to be blocked, last stderr: {}",
        last_stderr.unwrap_or_else(|| "<none>".to_string())
    );
}

#[cfg(unix)]
fn wait_for_instance_running(vault_dir: &std::path::Path) -> OkProcess {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    let mut last_status = None;

    while Instant::now() < deadline {
        let mut process = OkProcess::spawn(vault_dir);
        let stable_until = Instant::now() + RUNNING_STABILITY_WINDOW;

        loop {
            match process.try_wait().expect("should be able to check status") {
                None if Instant::now() >= stable_until => return process,
                None => {}
                Some(status) => {
                    last_status = Some(status);
                    break;
                }
            }

            if Instant::now() >= deadline {
                break;
            }

            thread::sleep(POLL_INTERVAL);
        }

        thread::sleep(POLL_INTERVAL);
    }

    panic!("timed out waiting for instance to keep running, last status: {last_status:?}");
}

/// Verify that a second `ok` process is blocked when the first holds the lock.
#[test]
#[cfg(unix)]
fn second_process_is_blocked_when_lock_is_held() {
    let dir = tempfile::tempdir().unwrap();
    let vault_dir = dir.path().to_path_buf();

    let mut first = OkProcess::spawn(&vault_dir);

    // Wait for the first process to create the lock file, then poll the public
    // startup path until it reports the single-instance lock error.
    wait_for_lock_file_created_by_process(&mut first, &vault_dir);
    let second = wait_for_second_instance_blocked(&mut first, &vault_dir);

    let stderr = String::from_utf8_lossy(&second.stderr);
    assert!(
        !second.status.success(),
        "second instance should exit with failure"
    );
    assert!(
        stderr.contains("already running"),
        "stderr should contain 'already running', got: {stderr}"
    );

    // `first` terminates the subprocess on drop, including panic paths.
}

/// Verify that after the first instance exits, a new one can acquire the lock.
#[test]
#[cfg(unix)]
fn lock_released_after_first_instance_exits() {
    let dir = tempfile::tempdir().unwrap();
    let vault_dir = dir.path().to_path_buf();

    // Start and then stop first instance
    let mut first = OkProcess::spawn(&vault_dir);
    wait_for_lock_file_created_by_process(&mut first, &vault_dir);
    wait_for_second_instance_blocked(&mut first, &vault_dir);
    first.terminate();

    // Second instance should be able to start and keep running.
    let mut second = wait_for_instance_running(&vault_dir);

    // Verify second instance is running (not blocked)
    let status = second.try_wait().expect("should be able to check status");
    assert!(
        status.is_none(),
        "second instance should still be running (lock was released)"
    );
}

#[test]
#[cfg(unix)]
fn sigterm_exits_running_tui_process() {
    let dir = tempfile::tempdir().unwrap();
    let vault_dir = dir.path().to_path_buf();
    let mut process = wait_for_instance_running(&vault_dir);
    process.wait_for_tui_output();

    let status = process
        .terminate_and_wait()
        .expect("process should produce an exit status after SIGTERM");

    assert!(
        status.success(),
        "SIGTERM should follow graceful shutdown and exit successfully, got {status}"
    );
}
