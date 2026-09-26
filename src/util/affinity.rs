//! `--affinity`: pin benchmarked processes to a set of CPUs.

use anyhow::{bail, ensure, Context, Result};

/// Parse a CPU list such as `2`, `0-3` or `0,2,4-5` into sorted, unique CPU
/// indices.
pub fn parse_cpu_list(list: &str) -> Result<Vec<usize>> {
    let mut cpus = Vec::new();
    for part in list.split(',').map(str::trim) {
        ensure!(!part.is_empty(), "empty entry in CPU list '{list}'");
        match part.split_once('-') {
            Some((first, last)) => {
                let first: usize = first
                    .trim()
                    .parse()
                    .with_context(|| format!("invalid CPU '{first}' in '{list}'"))?;
                let last: usize = last
                    .trim()
                    .parse()
                    .with_context(|| format!("invalid CPU '{last}' in '{list}'"))?;
                ensure!(first <= last, "invalid CPU range '{part}' in '{list}'");
                cpus.extend(first..=last);
            }
            None => cpus.push(
                part.parse()
                    .with_context(|| format!("invalid CPU '{part}' in '{list}'"))?,
            ),
        }
    }
    cpus.sort_unstable();
    cpus.dedup();
    Ok(cpus)
}

/// Check that the CPUs exist and that pinning is supported on this platform.
pub fn validate(cpus: &[usize]) -> Result<()> {
    if !cfg!(any(target_os = "linux", windows)) {
        bail!(
            "'--affinity' is only supported on Linux and Windows (this OS has no API to pin \
             a process to CPUs)"
        );
    }
    let available = configured_cpus();
    if let Some(&cpu) = cpus.iter().find(|&&cpu| cpu >= available) {
        bail!(
            "CPU {cpu} does not exist: this system has CPUs 0-{}",
            available - 1
        );
    }
    if cfg!(windows) && cpus.iter().any(|&cpu| cpu >= usize::BITS as usize) {
        bail!(
            "'--affinity' supports CPUs 0-{} on Windows",
            usize::BITS - 1
        );
    }
    Ok(())
}

/// Number of CPUs configured in the system (not just the ones perfratio may use).
fn configured_cpus() -> usize {
    #[cfg(unix)]
    {
        // SAFETY: sysconf is always safe to call.
        let n = unsafe { libc::sysconf(libc::_SC_NPROCESSORS_CONF) };
        if n > 0 {
            return n as usize;
        }
    }
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

/// Restrict the process spawned by `command` to `cpus` (Linux). The affinity
/// is set in the child between fork and exec, so the benchmarked program never
/// runs on other CPUs.
#[cfg(target_os = "linux")]
pub fn apply(command: &mut std::process::Command, cpus: &[usize]) {
    use std::os::unix::process::CommandExt;

    // SAFETY: an all-zero cpu_set_t is the empty set.
    let mut set: libc::cpu_set_t = unsafe { std::mem::zeroed() };
    for &cpu in cpus {
        // SAFETY: CPU_SET only writes into `set`; `cpu` was validated.
        unsafe { libc::CPU_SET(cpu, &mut set) };
    }
    // SAFETY: the closure runs between fork and exec and only calls the
    // async-signal-safe sched_setaffinity syscall on data captured by value.
    unsafe {
        command.pre_exec(move || {
            if libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &set) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

/// Affinity mask for `SetProcessAffinityMask` (Windows).
#[cfg(windows)]
pub fn windows_mask(cpus: &[usize]) -> usize {
    cpus.iter().fold(0, |mask, &cpu| mask | (1usize << cpu))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cpu_lists() {
        assert_eq!(parse_cpu_list("0").unwrap(), vec![0]);
        assert_eq!(parse_cpu_list("0-3").unwrap(), vec![0, 1, 2, 3]);
        assert_eq!(parse_cpu_list("5, 1,3-4").unwrap(), vec![1, 3, 4, 5]);
        assert_eq!(parse_cpu_list("2,2,1-2").unwrap(), vec![1, 2]);
    }

    #[test]
    fn rejects_invalid_cpu_lists() {
        for list in ["", "a", "3-1", "1,", "-2", "1-x"] {
            assert!(parse_cpu_list(list).is_err(), "{list}");
        }
    }

    #[cfg(any(target_os = "linux", windows))]
    #[test]
    fn rejects_cpus_that_do_not_exist() {
        let err = validate(&[100_000]).unwrap_err();
        assert!(err.to_string().contains("does not exist"));
        assert!(validate(&[0]).is_ok());
    }

    #[cfg(not(any(target_os = "linux", windows)))]
    #[test]
    fn is_unsupported_on_this_platform() {
        assert!(validate(&[0])
            .unwrap_err()
            .to_string()
            .contains("only supported"));
    }
}
