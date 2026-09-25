//! `--priority`: run benchmarked processes with a different scheduling
//! priority, to reduce preemption by other processes (`realtime`, `high`) or to
//! benchmark background workloads (`idle`).

use anyhow::{bail, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Priority {
    /// Inherit joulex's priority (default)
    #[default]
    Normal,
    /// Linux/macOS: nice -20. Windows: HIGH_PRIORITY_CLASS
    High,
    /// Linux: SCHED_IDLE. macOS: nice 19. Windows: IDLE_PRIORITY_CLASS
    Idle,
    /// Linux: SCHED_FIFO at the maximum priority. Windows:
    /// REALTIME_PRIORITY_CLASS. Not available on macOS.
    Realtime,
}

impl Priority {
    pub fn parse(value: &str) -> Result<Self> {
        let priority = match value {
            "normal" => Priority::Normal,
            "high" => Priority::High,
            "idle" => Priority::Idle,
            "realtime" => Priority::Realtime,
            _ => bail!("invalid priority '{value}' (normal, high, idle, realtime)"),
        };
        if priority == Priority::Realtime && !cfg!(any(target_os = "linux", windows)) {
            bail!(
                "'--priority realtime' is only supported on Linux and Windows \
                 (use 'high' for the highest nice value)"
            );
        }
        Ok(priority)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Priority::Normal => "normal",
            Priority::High => "high",
            Priority::Idle => "idle",
            Priority::Realtime => "realtime",
        }
    }
}

/// How to get the permission that `priority` needs, if it failed.
pub fn permission_hint(priority: Priority) -> Option<&'static str> {
    match priority {
        Priority::Normal | Priority::Idle => None,
        Priority::High | Priority::Realtime => Some(if cfg!(target_os = "linux") {
            "raising the priority requires the CAP_SYS_NICE capability: run joulex as root, \
             or grant it with 'sudo setcap cap_sys_nice+ep \"$(command -v joulex)\"'"
        } else if cfg!(windows) {
            "without administrator rights, Windows silently lowers 'realtime' to 'high'"
        } else {
            "raising the priority requires root: run joulex with sudo"
        }),
    }
}

/// Unix: set the priority in the child between fork and exec, so the
/// benchmarked program (and everything it starts) never runs with the old one.
#[cfg(unix)]
pub fn apply(command: &mut std::process::Command, priority: Priority) {
    use std::os::unix::process::CommandExt;

    if priority == Priority::Normal {
        return;
    }
    // SAFETY: the closure runs between fork and exec and only calls the
    // async-signal-safe setpriority / sched_setscheduler syscalls on values
    // captured by copy.
    unsafe {
        command.pre_exec(move || set_current_process_priority(priority));
    }
}

#[cfg(unix)]
fn set_current_process_priority(priority: Priority) -> std::io::Result<()> {
    let check = |ret: libc::c_int| {
        if ret == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    };
    // `setpriority`'s `which` argument has a different integer type on glibc
    // than on other libcs, hence the inferred casts below.

    match priority {
        Priority::Normal => Ok(()),
        // SAFETY: plain syscall on the calling (child) process.
        Priority::High => check(unsafe { libc::setpriority(libc::PRIO_PROCESS as _, 0, -20) }),
        #[cfg(target_os = "linux")]
        Priority::Idle => {
            let param = libc::sched_param { sched_priority: 0 };
            // SAFETY: plain syscall on the calling process; `param` is valid.
            check(unsafe { libc::sched_setscheduler(0, libc::SCHED_IDLE, &param) })
        }
        #[cfg(not(target_os = "linux"))]
        // SAFETY: plain syscall on the calling (child) process.
        Priority::Idle => check(unsafe { libc::setpriority(libc::PRIO_PROCESS as _, 0, 19) }),
        #[cfg(target_os = "linux")]
        Priority::Realtime => {
            // SAFETY: plain syscall without side effects.
            let max = unsafe { libc::sched_get_priority_max(libc::SCHED_FIFO) };
            let param = libc::sched_param {
                sched_priority: max,
            };
            // SAFETY: plain syscall on the calling process; `param` is valid.
            check(unsafe { libc::sched_setscheduler(0, libc::SCHED_FIFO, &param) })
        }
        // Rejected by `Priority::parse` on other Unix systems
        #[cfg(not(target_os = "linux"))]
        Priority::Realtime => Err(std::io::Error::from(std::io::ErrorKind::Unsupported)),
    }
}

/// Windows priority class for `SetPriorityClass`, `None` to keep the default.
#[cfg(windows)]
pub fn windows_priority_class(priority: Priority) -> Option<u32> {
    use windows_sys::Win32::System::Threading::{
        HIGH_PRIORITY_CLASS, IDLE_PRIORITY_CLASS, REALTIME_PRIORITY_CLASS,
    };
    match priority {
        Priority::Normal => None,
        Priority::High => Some(HIGH_PRIORITY_CLASS),
        Priority::Idle => Some(IDLE_PRIORITY_CLASS),
        Priority::Realtime => Some(REALTIME_PRIORITY_CLASS),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_priorities() {
        assert_eq!(Priority::parse("normal").unwrap(), Priority::Normal);
        assert_eq!(Priority::parse("high").unwrap(), Priority::High);
        assert_eq!(Priority::parse("idle").unwrap(), Priority::Idle);
        assert!(Priority::parse("fast").is_err());
        assert_eq!(
            Priority::parse("realtime").is_ok(),
            cfg!(any(target_os = "linux", windows))
        );
        for p in [Priority::Normal, Priority::High, Priority::Idle] {
            assert_eq!(Priority::parse(p.as_str()).unwrap(), p);
        }
    }

    #[test]
    fn only_raising_the_priority_needs_a_hint() {
        assert!(permission_hint(Priority::Normal).is_none());
        assert!(permission_hint(Priority::Idle).is_none());
        assert!(permission_hint(Priority::High).is_some());
        assert!(permission_hint(Priority::Realtime).is_some());
    }
}
