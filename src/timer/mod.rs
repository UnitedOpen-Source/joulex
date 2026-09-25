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

/// Execute the given command and return a timing summary
pub fn execute_and_measure(
    mut command: Command,
    affinity: Option<&[usize]>,
    priority: crate::util::priority::Priority,
    capture: bool,
    timeout: Option<std::time::Duration>,
) -> Result<TimerResult> {
    // On Linux the affinity is applied by the caller (pre_exec); on other
    // Unix systems --affinity is rejected during option validation. The
    // priority is also applied by the caller (pre_exec) on Unix.
    #[cfg(not(windows))]
    let _ = (affinity, priority);

    #[cfg(not(windows))]
    if timeout.is_some() {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        // Create the process in a suspended state so that we don't miss any cpu time between process creation and `CPUTimer` start.
        command.creation_flags(CREATE_SUSPENDED);
    }

    let wallclock_timer = WallClockTimer::start();
    let mut child = command.spawn()?;

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

    // --show-output-on-failure: drain stderr on another thread while stdout
    // is drained here, so that the child can't block on a full pipe
    let stderr_tail = child
        .stderr
        .take()
        .filter(|_| capture)
        .map(|stderr| std::thread::spawn(move || read_tail(stderr, CAPTURE_LIMIT)));
    let stdout_tail = match child.stdout.take() {
        Some(stdout) if capture => Some(read_tail(stdout, CAPTURE_LIMIT)),
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
    let (status, usage) = self::unix_timer::wait_with_rusage(&child)?;
    #[cfg(windows)]
    let status = child.wait()?;

    let time_real = wallclock_timer.stop();
    let timed_out = watchdog.is_some_and(|w| w.disarm());

    #[cfg(not(windows))]
    let (time_user, time_system, memory_usage_byte, counters) = (
        usage.user,
        usage.system,
        usage.max_rss_byte,
        Some(usage.counters),
    );
    #[cfg(windows)]
    let (time_user, time_system, memory_usage_byte, counters) = {
        let (user, system, memory) = cpu_timer.stop();
        (user, system, memory, None)
    };

    let captured = capture.then(|| CapturedOutput {
        stdout: stdout_tail.unwrap_or_default(),
        stderr: stderr_tail
            .and_then(|thread| thread.join().ok())
            .unwrap_or_default(),
    });

    Ok(TimerResult {
        time_real,
        time_user,
        time_system,
        memory_usage_byte,
        counters,
        status,
        captured,
        timed_out,
    })
}

#[test]
fn read_tail_keeps_the_last_bytes() {
    let data: Vec<u8> = (0..100_000u32).map(|i| (i % 251) as u8).collect();
    assert_eq!(read_tail(&data[..], 1000), &data[data.len() - 1000..]);
    assert_eq!(read_tail(&data[..10], 1000), &data[..10]);
    assert!(read_tail(&[][..], 1000).is_empty());
}
