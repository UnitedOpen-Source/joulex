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

/// Used to indicate the result of running a command
#[derive(Debug, Copy, Clone)]
pub struct TimerResult {
    pub time_real: Second,
    pub time_user: Second,
    pub time_system: Second,
    pub memory_usage_byte: u64,
    /// OS resource counters (Unix only)
    pub counters: Option<crate::benchmark::timing_result::ResourceCounters>,
    /// The exit status of the process
    pub status: ExitStatus,
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
) -> Result<TimerResult> {
    // On Linux the affinity is applied by the caller (pre_exec); on other
    // Unix systems --affinity is rejected during option validation.
    #[cfg(not(windows))]
    let _ = affinity;

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
    let cpu_timer = {
        // SAFETY: We created a suspended process
        unsafe { self::windows_timer::CPUTimer::start_suspended_process(&child) }
    };

    if let Some(output) = child.stdout.take() {
        // Handle CommandOutputPolicy::Pipe
        discard(output);
    }

    // On Unix, reap the child with wait4 to get the resource usage of exactly
    // this process tree (see unix_timer::wait_with_rusage).
    #[cfg(not(windows))]
    let (status, usage) = self::unix_timer::wait_with_rusage(&child)?;
    #[cfg(windows)]
    let status = child.wait()?;

    let time_real = wallclock_timer.stop();
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

    Ok(TimerResult {
        time_real,
        time_user,
        time_system,
        memory_usage_byte,
        counters,
        status,
    })
}
