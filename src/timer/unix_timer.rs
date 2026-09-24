#![cfg(not(windows))]

use std::convert::TryFrom;
use std::io;
use std::mem;

use crate::timer::CPUTimes;
use crate::util::units::Second;

#[derive(Debug, Copy, Clone)]
pub struct CPUInterval {
    /// Total amount of time spent executing in user mode
    pub user: Second,

    /// Total amount of time spent executing in kernel mode
    pub system: Second,
}

pub struct CPUTimer {
    start_cpu: CPUTimes,
}

impl CPUTimer {
    pub fn start() -> io::Result<Self> {
        Ok(CPUTimer {
            start_cpu: get_cpu_times()?,
        })
    }

    pub fn stop(&self) -> io::Result<(Second, Second, u64)> {
        let end_cpu = get_cpu_times()?;
        let cpu_interval = cpu_time_interval(&self.start_cpu, &end_cpu);
        Ok((
            cpu_interval.user,
            cpu_interval.system,
            end_cpu.memory_usage_byte,
        ))
    }
}

/// Read CPU execution times ('user' and 'system')
fn get_cpu_times() -> io::Result<CPUTimes> {
    use libc::{getrusage, rusage, RUSAGE_CHILDREN};

    // SAFETY: `rusage` is a plain C struct of integers, so the all-zero bit
    // pattern is a valid value.
    let mut result: rusage = unsafe { mem::zeroed() };

    // SAFETY: `result` is a valid, exclusively borrowed `rusage` that
    // getrusage fills in; RUSAGE_CHILDREN is a valid `who` argument.
    if unsafe { getrusage(RUSAGE_CHILDREN, &mut result) } != 0 {
        return Err(io::Error::last_os_error());
    }

    const MICROSEC_PER_SEC: i64 = 1000 * 1000;

    // Linux and *BSD return the value in KibiBytes, Darwin flavors in bytes
    let max_rss_byte = if cfg!(target_os = "macos") || cfg!(target_os = "ios") {
        result.ru_maxrss
    } else {
        result.ru_maxrss * 1024
    };

    #[allow(clippy::useless_conversion)]
    Ok(CPUTimes {
        user_usec: i64::from(result.ru_utime.tv_sec) * MICROSEC_PER_SEC
            + i64::from(result.ru_utime.tv_usec),
        system_usec: i64::from(result.ru_stime.tv_sec) * MICROSEC_PER_SEC
            + i64::from(result.ru_stime.tv_usec),
        memory_usage_byte: u64::try_from(max_rss_byte).unwrap_or(0),
    })
}

/// Compute the time intervals in between two `CPUTimes` snapshots
fn cpu_time_interval(start: &CPUTimes, end: &CPUTimes) -> CPUInterval {
    CPUInterval {
        user: ((end.user_usec - start.user_usec) as f64) * 1e-6,
        system: ((end.system_usec - start.system_usec) as f64) * 1e-6,
    }
}

#[cfg(test)]
use approx::assert_relative_eq;

#[test]
fn test_cpu_time_interval() {
    let t_a = CPUTimes {
        user_usec: 12345,
        system_usec: 54321,
        memory_usage_byte: 0,
    };

    let t_b = CPUTimes {
        user_usec: 20000,
        system_usec: 70000,
        memory_usage_byte: 0,
    };

    let t_zero = cpu_time_interval(&t_a, &t_a);
    assert!(t_zero.user.abs() < f64::EPSILON);
    assert!(t_zero.system.abs() < f64::EPSILON);

    let t_ab = cpu_time_interval(&t_a, &t_b);
    assert_relative_eq!(0.007655, t_ab.user);
    assert_relative_eq!(0.015679, t_ab.system);

    let t_ba = cpu_time_interval(&t_b, &t_a);
    assert_relative_eq!(-0.007655, t_ba.user);
    assert_relative_eq!(-0.015679, t_ba.system);
}

#[test]
fn test_get_cpu_times_succeeds() {
    let t = get_cpu_times().expect("getrusage(RUSAGE_CHILDREN) should succeed");
    assert!(t.user_usec >= 0);
    assert!(t.system_usec >= 0);
}
