//! Run metadata attached to the machine-readable exports (JSON, CSV).

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Result};
use serde::Serialize;

/// Information about the perfratio invocation that produced an export.
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
        let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
        // Device-tree boards (e.g. "Raspberry Pi 4 Model B Rev 1.4")
        let board = std::fs::read_to_string("/sys/firmware/devicetree/base/model").ok();
        linux_cpu_model(&cpuinfo, board.as_deref())
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

/// The CPU model from `/proc/cpuinfo`. On ARM, where there is no model name
/// (#188), fall back to the device-tree board model, then to the CPU
/// implementer and part numbers (like `lscpu`).
#[cfg(any(target_os = "linux", test))]
fn linux_cpu_model(cpuinfo: &str, device_tree_model: Option<&str>) -> Option<String> {
    let field = |name: &str| {
        cpuinfo
            .lines()
            .filter_map(|line| line.split_once(':'))
            .find(|(key, _)| key.trim() == name)
            .map(|(_, value)| value.trim())
            .filter(|value| !value.is_empty())
    };
    if let Some(model) = ["model name", "Model", "Hardware"]
        .iter()
        .find_map(|name| field(name))
    {
        return Some(model.to_string());
    }
    if let Some(board) = device_tree_model
        .map(|m| m.trim_end_matches('\0').trim())
        .filter(|m| !m.is_empty())
    {
        return Some(board.to_string());
    }
    let parse_hex = |value: &str| u32::from_str_radix(value.trim_start_matches("0x"), 16).ok();
    let implementer = field("CPU implementer").and_then(parse_hex)?;
    let part = field("CPU part").and_then(parse_hex);
    Some(arm_cpu_name(implementer, part))
}

/// Names for the MIDR implementer and part numbers of common ARM cores.
#[cfg(any(target_os = "linux", test))]
fn arm_cpu_name(implementer: u32, part: Option<u32>) -> String {
    let vendor = match implementer {
        0x41 => "ARM",
        0x42 => "Broadcom",
        0x43 => "Cavium",
        0x46 => "Fujitsu",
        0x48 => "HiSilicon",
        0x4e => "NVIDIA",
        0x51 => "Qualcomm",
        0x53 => "Samsung",
        0x61 => "Apple",
        0x6d => "Microsoft",
        0xc0 => "Ampere",
        _ => return format!("ARM CPU (implementer {implementer:#04x})"),
    };
    let core = match (implementer, part) {
        (0x41, Some(0xd03)) => Some("Cortex-A53"),
        (0x41, Some(0xd05)) => Some("Cortex-A55"),
        (0x41, Some(0xd07)) => Some("Cortex-A57"),
        (0x41, Some(0xd08)) => Some("Cortex-A72"),
        (0x41, Some(0xd0b)) => Some("Cortex-A76"),
        (0x41, Some(0xd0c)) => Some("Neoverse-N1"),
        (0x41, Some(0xd40)) => Some("Neoverse-V1"),
        (0x41, Some(0xd49)) => Some("Neoverse-N2"),
        (0x41, Some(0xd4f)) => Some("Neoverse-V2"),
        (0xc0, Some(0xac3)) => Some("Ampere-1"),
        _ => None,
    };
    match (core, part) {
        (Some(core), _) => format!("{vendor} {core}"),
        (None, Some(part)) => format!("{vendor} CPU (part {part:#05x})"),
        (None, None) => format!("{vendor} CPU"),
    }
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
            ["/home/alice/.cargo/bin/perfratio", "-N", "sleep 0.1"].map(std::ffi::OsString::from);
        assert_eq!(command_line(args), vec!["perfratio", "-N", "sleep 0.1"]);
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

#[test]
fn test_linux_cpu_model() {
    let x86 = "processor\t: 0\nvendor_id\t: GenuineIntel\nmodel name\t: Intel(R) Core(TM) i7-8550U CPU @ 1.80GHz\n";
    assert_eq!(
        linux_cpu_model(x86, None).as_deref(),
        Some("Intel(R) Core(TM) i7-8550U CPU @ 1.80GHz")
    );

    // aarch64 (e.g. an AWS Graviton2): no model name
    let arm = "processor\t: 0\nBogoMIPS\t: 243.75\nCPU implementer\t: 0x41\nCPU architecture: 8\nCPU part\t: 0xd0c\n";
    assert_eq!(
        linux_cpu_model(arm, None).as_deref(),
        Some("ARM Neoverse-N1")
    );
    // A device-tree board model wins over the core name
    assert_eq!(
        linux_cpu_model(arm, Some("Raspberry Pi 4 Model B Rev 1.4\0")).as_deref(),
        Some("Raspberry Pi 4 Model B Rev 1.4")
    );
    // Apple Silicon under virtualization reports part 0x000
    let apple = "CPU implementer\t: 0x61\nCPU part\t: 0x000\n";
    assert_eq!(
        linux_cpu_model(apple, None).as_deref(),
        Some("Apple CPU (part 0x000)")
    );
    assert_eq!(
        linux_cpu_model("CPU implementer\t: 0x99\n", None).as_deref(),
        Some("ARM CPU (implementer 0x99)")
    );
    assert_eq!(linux_cpu_model("", None), None);
    assert_eq!(linux_cpu_model("", Some("\0")), None);
}
