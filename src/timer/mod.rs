mod wall_clock_timer;

#[cfg(windows)]
mod windows_timer;

#[cfg(not(windows))]
mod unix_timer;

#[cfg(target_os = "linux")]
use nix::fcntl::{splice, SpliceFFlags};
#[cfg(target_os = "linux")]
use std::fs::File;
#[cfg(target_os = "linux")]
use std::os::fd::AsFd;

#[cfg(target_os = "windows")]
use windows_sys::Win32::System::Threading::CREATE_SUSPENDED;

#[cfg(not(windows))]
use std::os::unix::process::ExitStatusExt;

#[cfg(windows)]
use std::os::windows::process::ExitStatusExt;

use crate::util::units::Second;
use wall_clock_timer::WallClockTimer;

use std::io::Read;
use std::process::{ChildStdout, Command, ExitStatus};

use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

/// Used to indicate the result of running a command
#[derive(Debug, Clone)]
pub struct TimerResult {
    pub time_real: Second,
    pub time_total: Second,
    pub time_user: Second,
    pub time_system: Second,
    pub memory_usage_byte: u64,
    /// OS resource counters (Unix only)
    pub counters: Option<crate::benchmark::timing_result::ResourceCounters>,
    /// The exit status of the process
    pub status: ExitStatus,
    /// The tail of stdout and stderr (`--show-output-on-failure`)
    pub captured: Option<CapturedOutput>,
    /// Whether the process timed out and was killed by the watchdog
    pub timed_out: bool,
    /// Whether `--until` was active and matched the pattern
    pub until_matched: Option<bool>,
    /// Extracted custom metrics
    pub custom_metrics: std::collections::BTreeMap<String, f64>,
}

#[cfg(windows)]
#[derive(Clone, Copy)]
pub struct SendHandle(pub windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
// SAFETY: Win32 HANDLEs to Job Objects can be safely referenced across threads.
unsafe impl Send for SendHandle {}
#[cfg(windows)]
unsafe impl Sync for SendHandle {}

pub struct Watchdog {
    done: mpsc::Sender<()>,
    fired: Arc<AtomicBool>,
    handle: std::thread::JoinHandle<()>,
}

impl Watchdog {
    #[cfg(not(windows))]
    pub fn arm(pid: u32, timeout: std::time::Duration) -> Self {
        let (tx, rx) = mpsc::channel::<()>();
        let fired = Arc::new(AtomicBool::new(false));
        let f = fired.clone();
        let handle = std::thread::spawn(move || {
            if rx.recv_timeout(timeout).is_err() {
                f.store(true, Ordering::SeqCst);
                // SAFETY: plain syscall; negative pid targets the process group created via command.process_group(0).
                unsafe {
                    libc::kill(-(pid as libc::pid_t), libc::SIGKILL);
                }
            }
        });
        Watchdog {
            done: tx,
            fired,
            handle,
        }
    }

    #[cfg(windows)]
    pub fn arm(job_handle: SendHandle, timeout: std::time::Duration) -> Self {
        let (tx, rx) = mpsc::channel::<()>();
        let fired = Arc::new(AtomicBool::new(false));
        let f = fired.clone();
        let raw_handle = job_handle.0 as usize;
        let handle = std::thread::spawn(move || {
            if rx.recv_timeout(timeout).is_err() {
                f.store(true, Ordering::SeqCst);
                // SAFETY: TerminateJobObject terminates all processes associated with the job object.
                unsafe {
                    windows_sys::Win32::System::JobObjects::TerminateJobObject(
                        raw_handle as windows_sys::Win32::Foundation::HANDLE,
                        1,
                    );
                }
            }
        });
        Watchdog {
            done: tx,
            fired,
            handle,
        }
    }

    /// Disarm the watchdog and report whether the timeout fired.
    pub fn disarm(self) -> bool {
        let _ = self.done.send(());
        let _ = self.handle.join();
        self.fired.load(Ordering::SeqCst)
    }
}

#[cfg(not(windows))]
pub struct TerminateWatcher {
    done: mpsc::Sender<()>,
    handle: Option<std::thread::JoinHandle<()>>,
}

#[cfg(not(windows))]
impl TerminateWatcher {
    pub fn arm(pid: u32, timeout: std::time::Duration) -> Self {
        let (tx, rx) = mpsc::channel::<()>();
        let handle = std::thread::spawn(move || {
            if rx.recv_timeout(timeout).is_err() {
                // SAFETY: plain syscall; negative pid targets the process group created via command.process_group(0).
                unsafe {
                    libc::kill(-(pid as libc::pid_t), libc::SIGKILL);
                }
            }
        });
        TerminateWatcher {
            done: tx,
            handle: Some(handle),
        }
    }

    pub fn disarm(mut self) {
        let _ = self.done.send(());
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Ensure all processes in the process group are terminated safely.
///
/// After SIGTERM is delivered and the process group leader exits, descendants that
/// ignore SIGTERM may still be running. This gives well-behaved processes a brief
/// grace period to exit cleanly before forcefully killing any surviving descendants
/// with SIGKILL.
#[cfg(not(windows))]
fn terminate_process_group(pid: u32) {
    if pid <= 1 {
        return;
    }
    let pgid = -(pid as libc::pid_t);

    let is_group_alive = || -> bool {
        // SAFETY: kill with signal 0 checks for process existence without delivering a signal.
        let ret = unsafe { libc::kill(pgid, 0) };
        if ret == 0 {
            true
        } else {
            std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
        }
    };

    if !is_group_alive() {
        return;
    }

    // Give remaining processes that received SIGTERM a short chance to exit cleanly.
    for _ in 0..10 {
        std::thread::sleep(std::time::Duration::from_millis(5));
        if !is_group_alive() {
            return;
        }
    }

    // Forcefully kill any remaining processes in the group (e.g. descendants ignoring SIGTERM).
    // SAFETY: negative pid targets the process group created via command.process_group(0).
    unsafe {
        libc::kill(pgid, libc::SIGKILL);
    }

    // Wait until all processes in the group have exited.
    for _ in 0..50 {
        if !is_group_alive() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// The last bytes of a run's stdout and stderr (`--show-output-on-failure`)
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CapturedOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// How many bytes of each stream `--show-output-on-failure` keeps
pub const CAPTURE_LIMIT: usize = 64 << 10;

/// Read `input` to the end, keeping only its last `limit` bytes.
fn read_tail(mut input: impl Read, limit: usize) -> Vec<u8> {
    let mut tail = Vec::new();
    let mut buf = [0; 16 << 10];
    while let Ok(bytes) = input.read(&mut buf) {
        if bytes == 0 {
            break;
        }
        tail.extend_from_slice(&buf[..bytes]);
        // Trim in batches, so that each byte is moved at most twice
        if tail.len() > 2 * limit {
            tail.drain(..tail.len() - limit);
        }
    }
    if tail.len() > limit {
        tail.drain(..tail.len() - limit);
    }
    tail
}

/// Discard the output of a child process.
fn discard(output: ChildStdout) {
    const CHUNK_SIZE: usize = 64 << 10;

    #[cfg(target_os = "linux")]
    {
        if let Ok(file) = File::create("/dev/null") {
            while let Ok(bytes) = splice(
                output.as_fd(),
                None,
                file.as_fd(),
                None,
                CHUNK_SIZE,
                SpliceFFlags::empty(),
            ) {
                if bytes == 0 {
                    break;
                }
            }
        }
    }

    let mut output = output;
    let mut buf = [0; CHUNK_SIZE];
    while let Ok(bytes) = output.read(&mut buf) {
        if bytes == 0 {
            break;
        }
    }
}

/// Read from `out` until the `needle` byte pattern is found.
/// Preserves the last `needle.len() - 1` bytes across chunk boundaries
/// so matches split across reads are detected.
pub fn read_until(mut out: impl Read, needle: &[u8]) -> std::io::Result<bool> {
    if needle.is_empty() {
        return Ok(true);
    }
    let mut buf = [0u8; 64 << 10];
    let mut carry: Vec<u8> = Vec::new();
    loop {
        let n = out.read(&mut buf)?;
        if n == 0 {
            return Ok(false);
        }
        carry.extend_from_slice(&buf[..n]);
        if carry.windows(needle.len()).any(|w| w == needle) {
            return Ok(true);
        }
        let keep = (needle.len() - 1).min(carry.len());
        carry.drain(..carry.len() - keep);
    }
}

/// Fixed 16-byte representation of a monotonic timestamp passed from child to parent over pipe.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChildTimestamp {
    pub tv_sec: i64,
    pub tv_nsec: i64,
}

#[cfg(not(windows))]
pub fn timespec_diff_seconds(start: &libc::timespec, end: &libc::timespec) -> f64 {
    let sec_diff = (end.tv_sec as f64) - (start.tv_sec as f64);
    let nsec_diff = (end.tv_nsec as f64) - (start.tv_nsec as f64);
    (sec_diff + nsec_diff * 1e-9).max(0.0)
}

#[cfg(not(windows))]
pub fn current_monotonic_timespec() -> libc::timespec {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: ts points to a valid libc::timespec struct.
    let _ = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    ts
}

#[cfg(not(windows))]
pub fn read_child_timestamp(fd: libc::c_int) -> std::io::Result<libc::timespec> {
    let mut payload = ChildTimestamp {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let buf = &mut payload as *mut ChildTimestamp as *mut u8;
    let len = std::mem::size_of::<ChildTimestamp>();
    let mut read_bytes = 0;

    while read_bytes < len {
        // SAFETY: `fd` is a valid open file descriptor for reading; `buf` is a valid pointer
        // to `len` bytes in the stack frame.
        let ret = unsafe {
            libc::read(
                fd,
                buf.add(read_bytes) as *mut libc::c_void,
                len - read_bytes,
            )
        };
        if ret > 0 {
            read_bytes += ret as usize;
        } else if ret == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "unexpected EOF reading child timestamp from pipe",
            ));
        } else {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            return Err(err);
        }
    }

    Ok(libc::timespec {
        tv_sec: payload.tv_sec as _,
        tv_nsec: payload.tv_nsec as libc::c_long,
    })
}

#[cfg(any(target_os = "linux", test))]
#[cfg(not(windows))]
/// Write monotonic timestamp to pipe in pre_exec hook.
///
/// # Safety
///
/// This function must only invoke async-signal-safe syscalls
/// (`clock_gettime`, `write`, `close`, errno helper) and must not allocate heap memory.
pub unsafe fn pre_exec_write_timestamp(raw_write: libc::c_int) -> std::io::Result<()> {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    if libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) != 0 {
        #[cfg(target_os = "linux")]
        let err = *libc::__errno_location();
        #[cfg(not(target_os = "linux"))]
        let err = *libc::__error();
        return Err(std::io::Error::from_raw_os_error(err));
    }
    #[allow(clippy::unnecessary_cast)]
    let payload = ChildTimestamp {
        tv_sec: ts.tv_sec as i64,
        tv_nsec: ts.tv_nsec as i64,
    };
    let buf = &payload as *const ChildTimestamp as *const u8;
    let len = std::mem::size_of::<ChildTimestamp>();
    let mut written = 0;
    while written < len {
        let ret = libc::write(
            raw_write,
            buf.add(written) as *const libc::c_void,
            len - written,
        );
        if ret > 0 {
            written += ret as usize;
        } else if ret < 0 {
            #[cfg(target_os = "linux")]
            let err = *libc::__errno_location();
            #[cfg(not(target_os = "linux"))]
            let err = *libc::__error();
            if err == libc::EINTR {
                continue;
            }
            return Err(std::io::Error::from_raw_os_error(err));
        } else {
            return Err(std::io::Error::from_raw_os_error(libc::EIO));
        }
    }
    libc::close(raw_write);
    Ok(())
}

/// Execute the given command and return a timing summary
pub fn execute_and_measure(
    mut command: Command,
    affinity: Option<&[usize]>,
    priority: crate::util::priority::Priority,
    capture: bool,
    timeout: Option<std::time::Duration>,
    until: Option<&crate::options::UntilSettings>,
    stdout_capture_limit: Option<usize>,
) -> Result<TimerResult> {
    // On Linux the affinity is applied by the caller (pre_exec); on other
    // Unix systems --affinity is rejected during option validation. The
    // priority is also applied by the caller (pre_exec) on Unix.
    #[cfg(not(windows))]
    let _ = (affinity, priority);

    #[cfg(not(windows))]
    if timeout.is_some() || until.is_some() {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        // Create the process in a suspended state so that we don't miss any cpu time between process creation and `CPUTimer` start.
        command.creation_flags(CREATE_SUSPENDED);
    }

    #[cfg(target_os = "linux")]
    let (pipe_read, pipe_write) = {
        use std::os::fd::FromRawFd;
        let mut fds = [0i32; 2];
        // SAFETY: fds points to a valid 2-element i32 array.
        let ret = unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) };
        if ret != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        use std::os::fd::AsRawFd;
        use std::os::unix::process::CommandExt;
        // SAFETY: fds were created by pipe2 above and are valid open file descriptors.
        let (read_fd, write_fd) = unsafe {
            (
                std::os::fd::OwnedFd::from_raw_fd(fds[0]),
                std::os::fd::OwnedFd::from_raw_fd(fds[1]),
            )
        };
        let raw_write = write_fd.as_raw_fd();
        // SAFETY: pre_exec runs in child process between fork and execve.
        // It only calls async-signal-safe functions (clock_gettime, write, close)
        // on data captured by copy without heap allocation.
        unsafe {
            command.pre_exec(move || pre_exec_write_timestamp(raw_write));
        }
        (read_fd, write_fd)
    };

    let wallclock_timer = WallClockTimer::start();

    let mut child = command.spawn()?;

    #[cfg(target_os = "linux")]
    let child_start_ts = {
        drop(pipe_write);
        use std::os::fd::AsRawFd;
        match read_child_timestamp(pipe_read.as_raw_fd()) {
            Ok(ts) => ts,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error.into());
            }
        }
    };

    #[cfg(windows)]
    if let Some(cpus) = affinity {
        self::windows_timer::set_affinity(&child, crate::util::affinity::windows_mask(cpus))?;
    }

    #[cfg(windows)]
    if let Some(class) = crate::util::priority::windows_priority_class(priority) {
        self::windows_timer::set_priority_class(&child, class)?;
    }

    #[cfg(windows)]
    let cpu_timer = {
        // SAFETY: We created a suspended process
        unsafe { self::windows_timer::CPUTimer::start_suspended_process(&child) }
    };

    let watchdog = timeout.map(|t| {
        #[cfg(not(windows))]
        {
            Watchdog::arm(child.id(), t)
        }
        #[cfg(windows)]
        {
            Watchdog::arm(SendHandle(cpu_timer.raw_job_handle()), t)
        }
    });

    #[cfg(target_os = "linux")]
    let stop_timer = |wallclock: &WallClockTimer| -> (Second, Second) {
        let total = wallclock.stop();
        let end_ts = current_monotonic_timespec();
        (
            timespec_diff_seconds(&child_start_ts, &end_ts).min(total),
            total,
        )
    };

    #[cfg(not(target_os = "linux"))]
    let stop_timer = |wallclock: &WallClockTimer| -> (Second, Second) {
        let elapsed = wallclock.stop();
        (elapsed, elapsed)
    };

    let until_matched;
    let time_real;
    let time_total;
    let timed_out;
    let status;

    #[cfg(not(windows))]
    let (time_user, time_system, memory_usage_byte, counters);

    #[cfg(windows)]
    let (time_user, time_system, memory_usage_byte, counters);

    let captured;

    if let Some(until_settings) = until {
        let matched = if !until_settings.match_stderr {
            if let Some(mut stream) = child.stdout.take() {
                read_until(&mut stream, &until_settings.pattern)?
            } else {
                false
            }
        } else if let Some(mut stream) = child.stderr.take() {
            read_until(&mut stream, &until_settings.pattern)?
        } else {
            false
        };

        let (real, total) = stop_timer(&wallclock_timer);
        time_real = real;
        time_total = total;

        if matched {
            #[cfg(not(windows))]
            {
                let pid = child.id();
                // SAFETY: plain syscall; negative pid targets the process group created via command.process_group(0).
                unsafe {
                    libc::kill(-(pid as libc::pid_t), libc::SIGTERM);
                }
                let fallback = TerminateWatcher::arm(pid, std::time::Duration::from_secs(2));
                let (raw_status, usage) = self::unix_timer::wait_with_rusage(&child)?;
                fallback.disarm();
                terminate_process_group(pid);
                let _ = raw_status;
                time_user = usage.user;
                time_system = usage.system;
                memory_usage_byte = usage.max_rss_byte;
                counters = Some(usage.counters);
                status = ExitStatus::from_raw(0);
            }
            #[cfg(windows)]
            {
                // SAFETY: TerminateJobObject terminates all processes associated with the job object.
                unsafe {
                    windows_sys::Win32::System::JobObjects::TerminateJobObject(
                        cpu_timer.raw_job_handle(),
                        1,
                    );
                }
                let _ = child.wait()?;
                let (user, system, memory) = cpu_timer.stop();
                time_user = user;
                time_system = system;
                memory_usage_byte = memory;
                counters = None;
                status = ExitStatus::from_raw(0);
            }
            timed_out = watchdog.is_some_and(|w| w.disarm());
            until_matched = Some(true);
        } else {
            // Process closed pipe or exited without printing pattern
            #[cfg(not(windows))]
            {
                let (raw_status, usage) = self::unix_timer::wait_with_rusage(&child)?;
                time_user = usage.user;
                time_system = usage.system;
                memory_usage_byte = usage.max_rss_byte;
                counters = Some(usage.counters);
                timed_out = watchdog.is_some_and(|w| w.disarm());
                status = if !timed_out && raw_status.success() {
                    // Exited 0 without matching the required pattern -> mark as failure
                    ExitStatus::from_raw(1 << 8)
                } else {
                    raw_status
                };
            }
            #[cfg(windows)]
            {
                let raw_status = child.wait()?;
                let (user, system, memory) = cpu_timer.stop();
                time_user = user;
                time_system = system;
                memory_usage_byte = memory;
                counters = None;
                timed_out = watchdog.is_some_and(|w| w.disarm());
                status = if !timed_out && raw_status.success() {
                    // Exited 0 without matching the required pattern -> mark as failure
                    ExitStatus::from_raw(1)
                } else {
                    raw_status
                };
            }
            until_matched = Some(false);
        }
        captured = None;
    } else {
        // --show-output-on-failure: drain stderr on another thread while stdout
        // is drained here, so that the child can't block on a full pipe
        let stderr_tail = child
            .stderr
            .take()
            .filter(|_| capture)
            .map(|stderr| std::thread::spawn(move || read_tail(stderr, CAPTURE_LIMIT)));
        let stdout_tail = match child.stdout.take() {
            Some(stdout) if capture || stdout_capture_limit.is_some() => {
                let limit = stdout_capture_limit.unwrap_or(CAPTURE_LIMIT);
                Some(read_tail(stdout, limit))
            }
            // CommandOutputPolicy::Pipe
            Some(stdout) => {
                discard(stdout);
                None
            }
            None => None,
        };

        // On Unix, reap the child with wait4 to get the resource usage of exactly
        // this process tree (see unix_timer::wait_with_rusage).
        #[cfg(not(windows))]
        let (raw_status, usage) = self::unix_timer::wait_with_rusage(&child)?;
        #[cfg(windows)]
        let raw_status = child.wait()?;

        let (real, total) = stop_timer(&wallclock_timer);
        time_real = real;
        time_total = total;
        timed_out = watchdog.is_some_and(|w| w.disarm());
        status = raw_status;

        #[cfg(not(windows))]
        {
            time_user = usage.user;
            time_system = usage.system;
            memory_usage_byte = usage.max_rss_byte;
            counters = Some(usage.counters);
        }
        #[cfg(windows)]
        {
            let (user, system, memory) = cpu_timer.stop();
            time_user = user;
            time_system = system;
            memory_usage_byte = memory;
            counters = None;
        }

        captured = if capture || stdout_capture_limit.is_some() {
            Some(CapturedOutput {
                stdout: stdout_tail.unwrap_or_default(),
                stderr: stderr_tail
                    .and_then(|thread| thread.join().ok())
                    .unwrap_or_default(),
            })
        } else {
            None
        };
        until_matched = None;
    }

    Ok(TimerResult {
        time_real,
        time_total,
        time_user,
        time_system,
        memory_usage_byte,
        counters,
        status,
        captured,
        timed_out,
        until_matched,
        custom_metrics: std::collections::BTreeMap::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_tail_keeps_the_last_bytes() {
        let data: Vec<u8> = (0..100_000u32).map(|i| (i % 251) as u8).collect();
        assert_eq!(read_tail(&data[..], 1000), &data[data.len() - 1000..]);
        assert_eq!(read_tail(&data[..10], 1000), &data[..10]);
        assert!(read_tail(&[][..], 1000).is_empty());
    }

    struct ChunkReader<'a> {
        chunks: Vec<&'a [u8]>,
        index: usize,
    }

    impl<'a> Read for ChunkReader<'a> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.index >= self.chunks.len() {
                return Ok(0);
            }
            let chunk = self.chunks[self.index];
            self.index += 1;
            let len = chunk.len().min(buf.len());
            buf[..len].copy_from_slice(&chunk[..len]);
            Ok(len)
        }
    }

    #[test]
    fn test_read_until_exact_match() {
        let input = b"Hello world! Server is READY for connections.".as_slice();
        assert!(read_until(input, b"READY").unwrap());
    }

    #[test]
    fn test_read_until_split_across_reads() {
        let reader = ChunkReader {
            chunks: vec![b"Starting up system... REA", b"DY to serve"],
            index: 0,
        };
        assert!(read_until(reader, b"READY").unwrap());

        let reader2 = ChunkReader {
            chunks: vec![b"abc R", b"EADY xyz"],
            index: 0,
        };
        assert!(read_until(reader2, b"READY").unwrap());

        let reader3 = ChunkReader {
            chunks: vec![b"READ", b"Y!"],
            index: 0,
        };
        assert!(read_until(reader3, b"READY").unwrap());
    }

    #[test]
    fn test_read_until_not_found() {
        let input = b"Starting up... initializing... done.".as_slice();
        assert!(!read_until(input, b"READY").unwrap());
    }

    #[test]
    fn test_read_until_empty_needle() {
        let input = b"any data".as_slice();
        assert!(read_until(input, b"").unwrap());
    }

    #[test]
    fn test_read_until_empty_stream() {
        let input = b"".as_slice();
        assert!(!read_until(input, b"READY").unwrap());
    }

    #[test]
    fn test_read_until_partial_then_full() {
        let reader = ChunkReader {
            chunks: vec![b"REA", b"RE-", b"REA", b"READY!"],
            index: 0,
        };
        assert!(read_until(reader, b"READY").unwrap());
    }

    #[test]
    #[cfg(not(windows))]
    fn test_timespec_diff_seconds() {
        // Simple 1 second
        let t1 = libc::timespec {
            tv_sec: 10,
            tv_nsec: 0,
        };
        let t2 = libc::timespec {
            tv_sec: 11,
            tv_nsec: 0,
        };
        assert!((timespec_diff_seconds(&t1, &t2) - 1.0).abs() < 1e-9);

        // Nanosecond borrow / wrap-around
        let t3 = libc::timespec {
            tv_sec: 10,
            tv_nsec: 900_000_000,
        };
        let t4 = libc::timespec {
            tv_sec: 11,
            tv_nsec: 100_000_000,
        };
        assert!((timespec_diff_seconds(&t3, &t4) - 0.2).abs() < 1e-9);

        // Sub-millisecond execution (e.g. 500 microseconds)
        let t5 = libc::timespec {
            tv_sec: 100,
            tv_nsec: 100_000,
        };
        let t6 = libc::timespec {
            tv_sec: 100,
            tv_nsec: 600_000,
        };
        assert!((timespec_diff_seconds(&t5, &t6) - 0.0005).abs() < 1e-9);

        // Zero difference
        assert_eq!(timespec_diff_seconds(&t1, &t1), 0.0);

        // Reverse difference (monotonic clamp)
        assert_eq!(timespec_diff_seconds(&t2, &t1), 0.0);
    }

    #[test]
    #[cfg(not(windows))]
    fn test_pipe_timestamp_transfer_roundtrip() {
        let mut fds = [0i32; 2];
        // SAFETY: fds points to a valid 2-element i32 array.
        let ret = unsafe { libc::pipe(fds.as_mut_ptr()) };
        assert_eq!(ret, 0);

        let read_fd = fds[0];
        let write_fd = fds[1];

        let before = current_monotonic_timespec();
        // SAFETY: write_fd is a valid open file descriptor for writing.
        unsafe {
            pre_exec_write_timestamp(write_fd).unwrap();
        }

        let read_ts = read_child_timestamp(read_fd).unwrap();
        // SAFETY: read_fd is a valid open file descriptor.
        unsafe {
            libc::close(read_fd);
        }

        let diff = timespec_diff_seconds(&before, &read_ts);
        assert!(diff >= 0.0);
        assert!(diff < 1.0);
    }

    #[test]
    #[cfg(not(windows))]
    fn test_read_child_timestamp_chunked() {
        let mut fds = [0i32; 2];
        // SAFETY: fds points to a valid 2-element i32 array.
        let ret = unsafe { libc::pipe(fds.as_mut_ptr()) };
        assert_eq!(ret, 0);

        let read_fd = fds[0];
        let write_fd = fds[1];

        let payload = ChildTimestamp {
            tv_sec: 12345,
            tv_nsec: 67890,
        };
        // SAFETY: payload is a valid ChildTimestamp struct with size and alignment of ChildTimestamp.
        let bytes = unsafe {
            std::slice::from_raw_parts(
                &payload as *const ChildTimestamp as *const u8,
                std::mem::size_of::<ChildTimestamp>(),
            )
        };

        // Write 1 byte at a time to test partial read loop
        for &b in bytes {
            // SAFETY: write_fd is a valid file descriptor; &b is a valid 1-byte pointer.
            let n = unsafe { libc::write(write_fd, &b as *const u8 as *const libc::c_void, 1) };
            assert_eq!(n, 1);
        }
        // SAFETY: write_fd is a valid open file descriptor.
        unsafe {
            libc::close(write_fd);
        }

        let ts = read_child_timestamp(read_fd).unwrap();
        // SAFETY: read_fd is a valid open file descriptor.
        unsafe {
            libc::close(read_fd);
        }

        assert_eq!(ts.tv_sec, 12345);
        assert_eq!(ts.tv_nsec, 67890);
    }

    #[test]
    #[cfg(not(windows))]
    fn test_read_child_timestamp_unexpected_eof() {
        let mut fds = [0i32; 2];
        // SAFETY: fds points to a valid 2-element i32 array.
        let ret = unsafe { libc::pipe(fds.as_mut_ptr()) };
        assert_eq!(ret, 0);

        let read_fd = fds[0];
        let write_fd = fds[1];

        // Write fewer bytes than a full ChildTimestamp (e.g. 4 bytes instead of 16)
        let partial = [1u8, 2, 3, 4];
        // SAFETY: write_fd is valid; partial is a 4-byte slice.
        let n = unsafe {
            libc::write(
                write_fd,
                partial.as_ptr() as *const libc::c_void,
                partial.len(),
            )
        };
        assert_eq!(n, 4);
        // SAFETY: write_fd is a valid open file descriptor.
        unsafe {
            libc::close(write_fd);
        }

        let err = read_child_timestamp(read_fd).unwrap_err();
        // SAFETY: read_fd is a valid open file descriptor.
        unsafe {
            libc::close(read_fd);
        }

        assert_eq!(err.kind(), std::io::ErrorKind::UnexpectedEof);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn child_timestamp_excludes_pre_exec_delay() {
        use std::os::unix::process::CommandExt;

        let mut command = Command::new("true");
        // SAFETY: nanosleep is async-signal-safe and the closure only uses
        // stack values between fork and exec.
        unsafe {
            command.pre_exec(|| {
                let delay = libc::timespec {
                    tv_sec: 0,
                    tv_nsec: 80_000_000,
                };
                if libc::nanosleep(&delay, std::ptr::null_mut()) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }

        let result = execute_and_measure(
            command,
            None,
            crate::util::priority::Priority::Normal,
            false,
            None,
            None,
            None,
        )
        .unwrap();
        assert!(result.time_real >= 0.0);
        assert!(
            result.time_total - result.time_real >= 0.05,
            "full time should include the pre-exec delay: {result:?}"
        );
    }
}
