//! Ownership and bounded readiness checks for a child process.
use std::{
    ffi::OsStr,
    io::{self, Read, Write},
    process::{Child, ChildStdin, ChildStdout, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

#[derive(Debug)]
pub enum SupervisorError {
    Io(io::Error),
    Timeout,
    Cancelled,
    Eof,
    MessageTooLarge,
    InvalidJson(serde_json::Error),
}
impl From<io::Error> for SupervisorError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
impl std::fmt::Display for SupervisorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "child process I/O: {e}"),
            Self::Timeout => write!(f, "child process deadline expired"),
            Self::Cancelled => write!(f, "child process cancelled"),
            Self::Eof => write!(f, "child stdout closed before readiness"),
            Self::MessageTooLarge => write!(f, "child message exceeds limit"),
            Self::InvalidJson(e) => write!(f, "invalid child JSON: {e}"),
        }
    }
}
impl std::error::Error for SupervisorError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::InvalidJson(error) => Some(error),
            Self::Timeout | Self::Cancelled | Self::Eof | Self::MessageTooLarge => None,
        }
    }
}

/// Owns a child and reaps it on drop. Stderr is discarded to avoid pipe deadlocks.
pub struct ProcessSupervisor {
    child: Child,
    stdin: ChildStdin,
    stdout: ChildStdout,
    pending: Vec<u8>,
}

impl ProcessSupervisor {
    /// Spawn an explicit executable and argument list, without a shell.
    pub fn spawn<I, S>(executable: impl AsRef<OsStr>, args: I) -> io::Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = Command::new(executable);
        command
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        let mut child = command.spawn()?;
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        #[cfg(unix)]
        if let Err(error) = make_nonblocking(&stdout) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        Ok(Self {
            child,
            stdin,
            stdout,
            pending: Vec::new(),
        })
    }

    /// Send a small protocol command to the child.
    pub fn send(&mut self, value: &serde_json::Value) -> io::Result<()> {
        serde_json::to_writer(&mut self.stdin, value)?;
        self.stdin.write_all(b"\n")?;
        self.stdin.flush()
    }

    pub fn id(&self) -> u32 {
        self.child.id()
    }

    /// Read newline-delimited JSON until `is_ready` matches. The limit excludes the newline.
    /// Any error terminates and reaps the child.
    pub fn wait_for_ready(
        &mut self,
        deadline: Instant,
        max_message_bytes: usize,
        mut is_ready: impl FnMut(&serde_json::Value) -> bool,
    ) -> Result<serde_json::Value, SupervisorError> {
        self.wait_for_ready_cancelled(deadline, max_message_bytes, &mut is_ready, || false)
    }
    /// As above, but checks cancellation while waiting and reaps the owned child.
    pub fn wait_for_ready_cancelled(
        &mut self,
        deadline: Instant,
        max_message_bytes: usize,
        mut is_ready: impl FnMut(&serde_json::Value) -> bool,
        cancelled: impl Fn() -> bool,
    ) -> Result<serde_json::Value, SupervisorError> {
        let result = self.read_ready(deadline, max_message_bytes, &mut is_ready, &cancelled);
        if result.is_err() {
            let _ = self.kill_and_reap();
        }
        result
    }
    fn read_ready(
        &mut self,
        deadline: Instant,
        max: usize,
        is_ready: &mut impl FnMut(&serde_json::Value) -> bool,
        cancelled: &impl Fn() -> bool,
    ) -> Result<serde_json::Value, SupervisorError> {
        loop {
            if cancelled() {
                return Err(SupervisorError::Cancelled);
            }
            if Instant::now() >= deadline {
                return Err(SupervisorError::Timeout);
            }
            if let Some(end) = self.pending.iter().position(|&b| b == b'\n') {
                if end > max {
                    return Err(SupervisorError::MessageTooLarge);
                }
                let line: Vec<_> = self.pending.drain(..=end).collect();
                let value =
                    serde_json::from_slice(&line[..end]).map_err(SupervisorError::InvalidJson)?;
                if is_ready(&value) {
                    return Ok(value);
                }
                continue;
            }
            if self.pending.len() > max {
                return Err(SupervisorError::MessageTooLarge);
            }
            let mut buf = [0; 4096];
            // Read at most one byte beyond the limit so memory stays bounded.
            let count = buf
                .len()
                .min(max.saturating_sub(self.pending.len()).saturating_add(1));
            match read_available(&mut self.stdout, &mut buf[..count]) {
                Ok(0) => return Err(SupervisorError::Eof),
                Ok(n) => self.pending.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => thread::sleep(
                    deadline
                        .saturating_duration_since(Instant::now())
                        .min(Duration::from_millis(5)),
                ),
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e.into()),
            }
        }
    }
    fn kill_and_reap(&mut self) -> io::Result<ExitStatus> {
        if let Some(status) = self.child.try_wait()? {
            return Ok(status);
        }
        // The child may exit between try_wait and kill; wait still reaps it.
        let killed = self.child.kill();
        let status = self.child.wait();
        match status {
            Ok(status) => Ok(status),
            Err(e) => {
                killed?;
                Err(e)
            }
        }
    }
    /// Allow normal exit until the deadline, then kill and reap the child.
    pub fn stop(&mut self, deadline: Instant) -> io::Result<ExitStatus> {
        loop {
            if let Some(status) = self.child.try_wait()? {
                return Ok(status);
            }
            if Instant::now() >= deadline {
                return self.kill_and_reap();
            }
            thread::sleep(
                deadline
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(5)),
            );
        }
    }
}
impl Drop for ProcessSupervisor {
    fn drop(&mut self) {
        let _ = self.kill_and_reap();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    fn spawn(script: &str) -> ProcessSupervisor {
        ProcessSupervisor::spawn("/bin/sh", ["-c", script]).unwrap()
    }
    fn deadline() -> Instant {
        Instant::now() + Duration::from_secs(2)
    }
    #[test]
    fn wrapped_errors_preserve_their_sources() {
        use std::error::Error;
        let io = SupervisorError::Io(io::Error::new(io::ErrorKind::PermissionDenied, "denied"));
        assert_eq!(
            io.source()
                .unwrap()
                .downcast_ref::<io::Error>()
                .unwrap()
                .kind(),
            io::ErrorKind::PermissionDenied
        );
        let json = serde_json::from_str::<serde_json::Value>("{").unwrap_err();
        let invalid = SupervisorError::InvalidJson(json);
        assert!(invalid.source().unwrap().is::<serde_json::Error>());
        assert!(SupervisorError::Timeout.source().is_none());
    }

    #[test]
    fn ready_and_normal_exit() {
        let mut child = spawn("printf '{\"state\":\"READY\"}\\n'");
        let value = child
            .wait_for_ready(deadline(), 128, |v| v["state"] == "READY")
            .unwrap();
        assert_eq!(value["state"], "READY");
        assert!(child.stop(deadline()).unwrap().success());
    }
    #[test]
    fn malformed_and_eof_and_limit() {
        for (script, expected) in [
            ("printf 'bad\\n'", 0),
            ("exit 0", 1),
            ("printf '123456789'", 2),
        ] {
            let mut child = spawn(script);
            let err = child.wait_for_ready(deadline(), 4, |_| false).unwrap_err();
            assert!(matches!(
                (expected, err),
                (0, SupervisorError::InvalidJson(_))
                    | (1, SupervisorError::Eof)
                    | (2, SupervisorError::MessageTooLarge)
            ));
        }
    }
    #[test]
    fn cancellation_kills_and_reaps_owned_child() {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };
        let mut child = spawn("exec sleep 30");
        let pid = child.id();
        let cancelled = Arc::new(AtomicBool::new(false));
        let signal = cancelled.clone();
        let sender = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            signal.store(true, Ordering::Release);
        });
        let start = Instant::now();
        assert!(matches!(
            child.wait_for_ready_cancelled(
                start + Duration::from_secs(10),
                128,
                |_| true,
                || cancelled.load(Ordering::Acquire)
            ),
            Err(SupervisorError::Cancelled)
        ));
        sender.join().unwrap();
        assert!(start.elapsed() < Duration::from_secs(2));
        unsafe extern "C" {
            fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32;
        }
        let mut status = 0;
        // SAFETY: status is writable and pid is the child that was just reaped.
        assert_eq!(unsafe { waitpid(pid as i32, &mut status, 1) }, -1);
        assert_eq!(io::Error::last_os_error().raw_os_error(), Some(10));
    }
    #[test]
    fn timeout_kills_and_reaps() {
        let mut child = spawn("exec sleep 30");
        let pid = child.id();
        let start = Instant::now();
        assert!(matches!(
            child.wait_for_ready(start + Duration::from_millis(100), 128, |_| true),
            Err(SupervisorError::Timeout)
        ));
        assert!(start.elapsed() < Duration::from_secs(2));
        // waitpid on a reaped child reports ECHILD.
        unsafe extern "C" {
            fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32;
        }
        let mut status = 0;
        // SAFETY: status points to writable storage; pid identifies our child.
        assert_eq!(unsafe { waitpid(pid as i32, &mut status, 1) }, -1);
        assert_eq!(io::Error::last_os_error().raw_os_error(), Some(10)); // ECHILD on Linux; macOS uses 10 too.
    }
}

#[cfg(unix)]
fn make_nonblocking(stdout: &ChildStdout) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    // F_SETFL preserves existing flags and enables deadline-aware reads.
    unsafe extern "C" {
        fn fcntl(fd: i32, cmd: i32, ...) -> i32;
    }
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    const NONBLOCK: i32 = 0x4;
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    const NONBLOCK: i32 = 0x800;
    let fd = stdout.as_raw_fd();
    // SAFETY: fd is owned and valid for the lifetime of stdout; fcntl takes integer arguments.
    let result = unsafe {
        let flags = fcntl(fd, 3);
        if flags < 0 {
            -1
        } else {
            fcntl(fd, 4, flags | NONBLOCK)
        }
    };
    if result < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

// Peek before a synchronous anonymous-pipe read: ReadFile otherwise blocks past
// the operation deadline when the helper has not produced a complete response.
#[cfg(windows)]
fn read_available(stdout: &mut ChildStdout, buf: &mut [u8]) -> io::Result<usize> {
    use std::os::windows::io::AsRawHandle;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn PeekNamedPipe(
            pipe: *mut std::ffi::c_void,
            buffer: *mut std::ffi::c_void,
            buffer_size: u32,
            bytes_read: *mut u32,
            available: *mut u32,
            remaining: *mut u32,
        ) -> i32;
    }
    let mut available = 0u32;
    // SAFETY: the pipe handle remains owned by stdout and available is writable.
    let ok = unsafe {
        PeekNamedPipe(
            stdout.as_raw_handle(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            &mut available,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        let error = io::Error::last_os_error();
        return if error.raw_os_error() == Some(109) {
            Ok(0)
        } else {
            Err(error)
        };
    }
    if available == 0 {
        return Err(io::ErrorKind::WouldBlock.into());
    }
    let count = buf.len().min(available as usize);
    stdout.read(&mut buf[..count])
}
#[cfg(not(windows))]
fn read_available(stdout: &mut ChildStdout, buf: &mut [u8]) -> io::Result<usize> {
    stdout.read(buf)
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;
    #[test]
    fn readiness_and_deadline_reap_windows_children() {
        let mut child = ProcessSupervisor::spawn("cmd.exe", ["/D", "/C", "echo true"]).unwrap();
        let value = child
            .wait_for_ready(Instant::now() + Duration::from_secs(5), 1024, |v| v == true)
            .unwrap();
        assert_eq!(value, true);
        assert!(child
            .stop(Instant::now() + Duration::from_secs(2))
            .unwrap()
            .success());
        let mut child = ProcessSupervisor::spawn(
            "powershell.exe",
            [
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Seconds 30",
            ],
        )
        .unwrap();
        assert!(matches!(
            child.wait_for_ready(Instant::now() + Duration::from_millis(100), 1024, |_| true),
            Err(SupervisorError::Timeout)
        ));
        assert!(child.child.try_wait().unwrap().is_some());
        let mut child = ProcessSupervisor::spawn(
            "powershell.exe",
            [
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Seconds 30",
            ],
        )
        .unwrap();
        assert!(matches!(
            child.wait_for_ready_cancelled(
                Instant::now() + Duration::from_secs(5),
                1024,
                |_| true,
                || true
            ),
            Err(SupervisorError::Cancelled)
        ));
        assert!(child.child.try_wait().unwrap().is_some());
    }
}
