#![cfg(not(windows))]

use std::io;
use std::mem;
use std::os::unix::process::ExitStatusExt;
use std::process::{Child, ExitStatus};

use crate::util::units::Second;

/// Resource usage of a single benchmarked process (and the descendants it
/// waited for), as reported by `wait4`.
#[derive(Debug, Default, Copy, Clone, PartialEq)]
pub struct ChildUsage {
    /// Time spent executing in user mode
    pub user: Second,

    /// Time spent executing in kernel mode
    pub system: Second,

    /// Maximum resident set size, in bytes
    pub max_rss_byte: u64,
}

impl From<&libc::rusage> for ChildUsage {
    fn from(ru: &libc::rusage) -> Self {
        #[allow(clippy::useless_conversion)]
        let seconds =
            |t: libc::timeval| i64::from(t.tv_sec) as f64 + i64::from(t.tv_usec) as f64 * 1e-6;

        // Linux and *BSD report ru_maxrss in KiB, Darwin flavors in bytes
        let max_rss = u64::try_from(ru.ru_maxrss).unwrap_or(0);
        let max_rss_byte = if cfg!(any(target_os = "macos", target_os = "ios")) {
            max_rss
        } else {
            max_rss.saturating_mul(1024)
        };

        ChildUsage {
            user: seconds(ru.ru_utime),
            system: seconds(ru.ru_stime),
            max_rss_byte,
        }
    }
}

/// Wait for `child` to exit and return its exit status together with the
/// resource usage of that process and of all descendants it waited for (e.g.
/// the command started by an intermediate shell).
///
/// Unlike before/after snapshots of `getrusage(RUSAGE_CHILDREN)`, this gives
/// per-run values: in particular `ru_maxrss` of RUSAGE_CHILDREN is the maximum
/// over *all* children ever reaped, so peak memory used to leak across runs,
/// benchmarks and setup/prepare commands (#46).
///
/// The child is reaped by this call, so `child.wait()` must not be called
/// afterwards.
pub fn wait_with_rusage(child: &Child) -> io::Result<(ExitStatus, ChildUsage)> {
    let pid = libc::pid_t::try_from(child.id())
        .map_err(|_| io::Error::other("child process id does not fit into pid_t"))?;
    let mut status: libc::c_int = 0;
    // SAFETY: `rusage` is a plain C struct of integers, so the all-zero bit
    // pattern is a valid value.
    let mut usage: libc::rusage = unsafe { mem::zeroed() };

    loop {
        // SAFETY: `pid` refers to our own child, which has not been reaped yet
        // (the caller never calls `Child::wait`); `status` and `usage` are
        // valid, exclusively borrowed out-pointers.
        let ret = unsafe { libc::wait4(pid, &mut status, 0, &mut usage) };
        if ret == pid {
            return Ok((ExitStatus::from_raw(status), ChildUsage::from(&usage)));
        }
        let err = io::Error::last_os_error();
        if err.kind() != io::ErrorKind::Interrupted {
            return Err(err);
        }
    }
}

#[cfg(test)]
// The children are reaped by `wait_with_rusage` (wait4), which clippy can't see.
#[allow(clippy::zombie_processes)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn reports_exit_status_and_cpu_time() {
        let child = Command::new("sh")
            .args([
                "-c",
                "i=0; while [ $i -lt 20000 ]; do i=$((i+1)); done; exit 3",
            ])
            .spawn()
            .unwrap();
        let (status, usage) = wait_with_rusage(&child).unwrap();
        assert_eq!(status.code(), Some(3));
        assert!(usage.user + usage.system > 0.0);
        assert!(usage.max_rss_byte > 0);
    }

    #[test]
    fn peak_memory_is_per_process() {
        // A memory-hungry child followed by a tiny one: the second one must
        // not inherit the first one's peak RSS.
        let big = Command::new("dd")
            .args(["if=/dev/zero", "of=/dev/null", "bs=104857600", "count=1"])
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let (_, big_usage) = wait_with_rusage(&big).unwrap();

        let small = Command::new("true").spawn().unwrap();
        let (_, small_usage) = wait_with_rusage(&small).unwrap();

        assert!(big_usage.max_rss_byte > 90 * 1024 * 1024, "{big_usage:?}");
        assert!(
            small_usage.max_rss_byte < 50 * 1024 * 1024,
            "{small_usage:?}"
        );
    }
}
