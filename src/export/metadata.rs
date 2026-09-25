//! Run metadata attached to the machine-readable exports (JSON, CSV).

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Result};
use serde::Serialize;

/// Information about the joulex invocation that produced an export.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ExportMetadata {
    pub version: &'static str,
    pub command_line: Vec<String>,
    /// Start of the run, RFC 3339 in UTC
    pub started_at: String,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,
    pub system: SystemInfo,
}

/// Environment the benchmarks ran in. Deliberately no hostname or user name.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SystemInfo {
    pub os: &'static str,
    pub arch: &'static str,
    pub cpus: usize,
    /// CPU model name, if it can be determined
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_model: Option<String>,
    /// Kernel / OS release (e.g. "6.8.0-45-generic", "24.1.0")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kernel: Option<String>,
}

impl ExportMetadata {
    pub fn new(labels: BTreeMap<String, String>) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        ExportMetadata {
            version: env!("CARGO_PKG_VERSION"),
            command_line: command_line(std::env::args_os()),
            started_at: format_rfc3339_utc(now),
            labels,
            system: SystemInfo {
                os: std::env::consts::OS,
                arch: std::env::consts::ARCH,
                cpus: std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(1),
                cpu_model: cpu_model(),
                kernel: kernel_release(),
            },
        }
    }
}

/// The CPU model, e.g. "Intel(R) Core(TM) i7-8550U CPU @ 1.80GHz" or "Apple M2".
fn cpu_model() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").ok()?;
        cpuinfo
            .lines()
            .filter_map(|line| line.split_once(':'))
            .find(|(key, _)| matches!(key.trim(), "model name" | "Model" | "Hardware"))
            .map(|(_, value)| value.trim().to_string())
            .filter(|model| !model.is_empty())
    }
    #[cfg(target_os = "macos")]
    {
        sysctl_string(c"machdep.cpu.brand_string")
    }
    #[cfg(windows)]
    {
        std::env::var("PROCESSOR_IDENTIFIER").ok()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        None
    }
}

#[cfg(target_os = "macos")]
fn sysctl_string(name: &std::ffi::CStr) -> Option<String> {
    let mut buf = [0u8; 256];
    let mut len = buf.len();
    // SAFETY: `name` is NUL-terminated; `buf` has room for `len` bytes, and
    // sysctlbyname writes at most that many and updates `len`.
    let ret = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            buf.as_mut_ptr().cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if ret != 0 {
        return None;
    }
    let value = std::ffi::CStr::from_bytes_until_nul(&buf[..len.min(buf.len())]).ok()?;
    Some(value.to_string_lossy().trim().to_string()).filter(|s| !s.is_empty())
}

/// The kernel release from `uname`, on Unix.
fn kernel_release() -> Option<String> {
    #[cfg(unix)]
    {
        // SAFETY: an all-zero utsname is valid, and uname fills it in.
        let mut name: libc::utsname = unsafe { std::mem::zeroed() };
        // SAFETY: `name` is a valid, writable utsname.
        if unsafe { libc::uname(&mut name) } != 0 {
            return None;
        }
        // SAFETY: uname NUL-terminates the fields.
        let release = unsafe { std::ffi::CStr::from_ptr(name.release.as_ptr()) };
        Some(release.to_string_lossy().into_owned())
    }
    #[cfg(not(unix))]
    {
        None
    }
}

/// The command line, with the program reduced to its file name so that the
/// export doesn't reveal the installation path (e.g. a home directory).
fn command_line(args: impl IntoIterator<Item = std::ffi::OsString>) -> Vec<String> {
    let mut args: Vec<String> = args
        .into_iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    if let Some(program) = args.first_mut() {
        if let Some(name) = std::path::Path::new(program.as_str()).file_name() {
            *program = name.to_string_lossy().into_owned();
        }
    }
    args
}

/// Parse `--label KEY=VALUE` arguments. Keys may contain letters, digits,
/// `_`, `.` and `-`; duplicate keys are rejected.
pub fn parse_labels<'a>(
    args: impl IntoIterator<Item = &'a str>,
) -> Result<BTreeMap<String, String>> {
    let mut labels = BTreeMap::new();
    for arg in args {
        let Some((key, value)) = arg.split_once('=') else {
            bail!("Invalid label '{arg}': expected KEY=VALUE");
        };
        if key.is_empty()
            || !key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
        {
            bail!("Invalid label key '{key}': use letters, digits, '_', '.' or '-'");
        }
        if labels.insert(key.to_string(), value.to_string()).is_some() {
            bail!("Duplicate label key '{key}'");
        }
    }
    Ok(labels)
}

/// Format seconds since the Unix epoch as `YYYY-MM-DDTHH:MM:SSZ`.
fn format_rfc3339_utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);

    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);

    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_rfc3339() {
        assert_eq!(format_rfc3339_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_rfc3339_utc(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(format_rfc3339_utc(1_790_000_000), "2026-09-21T14:13:20Z");
        assert_eq!(format_rfc3339_utc(4_102_444_799), "2099-12-31T23:59:59Z");
    }

    #[test]
    fn command_line_strips_the_program_path() {
        let args =
            ["/home/alice/.cargo/bin/joulex", "-N", "sleep 0.1"].map(std::ffi::OsString::from);
        assert_eq!(command_line(args), vec!["joulex", "-N", "sleep 0.1"]);
    }

    #[test]
    fn parses_labels() {
        let labels = parse_labels(["commit=abc123", "runner=graviton-3", "empty="]).unwrap();
        assert_eq!(labels["commit"], "abc123");
        assert_eq!(labels["runner"], "graviton-3");
        assert_eq!(labels["empty"], "");

        let labels = parse_labels(["url=https://x.y/?a=b"]).unwrap();
        assert_eq!(labels["url"], "https://x.y/?a=b");
    }

    #[test]
    fn rejects_invalid_labels() {
        assert!(parse_labels(["novalue"]).is_err());
        assert!(parse_labels(["=value"]).is_err());
        assert!(parse_labels(["bad key=1"]).is_err());
        assert!(parse_labels(["a=1", "a=2"]).is_err());
    }
}
