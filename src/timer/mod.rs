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
        let handle = std::thread::spawn(move || {
            if rx.recv_timeout(timeout).is_err() {
                f.store(true, Ordering::SeqCst);
                // SAFETY: TerminateJobObject terminates all processes associated with the job object.
                unsafe {
                    windows_sys::Win32::System::JobObjects::TerminateJobObject(job_handle.0, 1);
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
        if ret == 0 {
            use std::os::fd::AsRawFd;
            use std::os::unix::process::CommandExt;
            // SAFETY: fds were created by pipe2 above.
            let read_fd = unsafe { std::os::fd::OwnedFd::from_raw_fd(fds[0]) };
            let write_fd = unsafe { std::os::fd::OwnedFd::from_raw_fd(fds[1]) };
            let raw_write = write_fd.as_raw_fd();
            unsafe {
                command.pre_exec(move || {
                    let _ = raw_write;
                    Ok(())
                });
            }
            (Some(read_fd), Some(write_fd))
        } else {
            (None, None)
        }
    };

    #[cfg(not(target_os = "linux"))]
    let wallclock_timer = WallClockTimer::start();

    let mut child = command.spawn()?;

    #[cfg(target_os = "linux")]
    let wallclock_timer = {
        if let (Some(read_fd), Some(write_fd)) = (pipe_read, pipe_write) {
            use std::os::fd::AsRawFd;
            drop(write_fd);
            let mut b = [0u8; 1];
            // SAFETY: read_fd is a valid file descriptor; b points to a valid 1-byte buffer.
            let _ = unsafe { libc::read(read_fd.as_raw_fd(), b.as_mut_ptr().cast(), 1) };
            drop(read_fd);
        }
        WallClockTimer::start()
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

    let until_matched;
    let time_real;
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

        time_real = wallclock_timer.stop();

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

        time_real = wallclock_timer.stop();
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
}
