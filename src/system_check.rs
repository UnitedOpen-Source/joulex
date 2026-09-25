//! `--check-system`: report environment conditions that make benchmark results
//! noisy or irreproducible (CPU frequency scaling, turbo boost, load, battery,
//! thermal throttling) before benchmarking, with a hint for each problem.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::output::colors;

/// Hidden testing hook: read the Linux `/proc` and `/sys` files below this
/// directory instead of `/`, on any platform.
pub const FAKE_ROOT_ENV: &str = "JOULEX_SYSTEM_CHECK_ROOT";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Nothing to worry about
    Ok,
    /// A condition that often adds noise (load, battery, heat)
    Warn,
    /// A setting that is known to add noise (frequency scaling, turbo boost)
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub name: &'static str,
    pub status: Status,
    pub detail: String,
    pub hint: Option<String>,
}

impl Check {
    fn new(name: &'static str, status: Status, detail: impl Into<String>) -> Self {
        Check {
            name,
            status,
            detail: detail.into(),
            hint: None,
        }
    }

    fn with_hint(mut self, hint: impl Into<String>) -> Self {
        if self.status != Status::Ok {
            self.hint = Some(hint.into());
        }
        self
    }
}

impl fmt::Display for Check {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let symbol = match self.status {
            Status::Ok => colors::green("✔"),
            Status::Warn => colors::yellow("!"),
            Status::Fail => colors::red("✖"),
        };
        write!(f, "  {symbol} {:<16} {}", self.name, self.detail)?;
        if let Some(hint) = &self.hint {
            write!(f, "  → {hint}")?;
        }
        Ok(())
    }
}

/// Run all checks available on this platform.
pub fn run_checks() -> Vec<Check> {
    let cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    if let Some(root) = std::env::var_os(FAKE_ROOT_ENV) {
        return linux_checks(Path::new(&root), cpus);
    }
    #[cfg(target_os = "linux")]
    {
        linux_checks(Path::new("/"), cpus)
    }
    #[cfg(target_os = "macos")]
    {
        macos_checks(cpus)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = cpus;
        Vec::new()
    }
}

/// The report shown before benchmarking.
pub fn report(checks: &[Check]) -> String {
    let mut out = String::from("System check:\n");
    if checks.is_empty() {
        out.push_str("  (no checks are available on this platform)\n");
    }
    for check in checks {
        out.push_str(&format!("{check}\n"));
    }
    out
}

/// Whether every check passed.
pub fn all_passed(checks: &[Check]) -> bool {
    checks.iter().all(|c| c.status == Status::Ok)
}

// ---------------------------------------------------------------------------
// Linux (sysfs / procfs below `root`)

fn read(root: &Path, path: &str) -> Option<String> {
    std::fs::read_to_string(root.join(path.trim_start_matches('/')))
        .ok()
        .map(|s| s.trim().to_string())
}

/// Entries of a directory below `root` whose name starts with `prefix`, sorted.
fn entries(root: &Path, dir: &str, prefix: &str) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(root.join(dir.trim_start_matches('/')))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(prefix))
        .map(|e| e.path())
        .collect();
    paths.sort();
    paths
}

fn linux_checks(root: &Path, cpus: usize) -> Vec<Check> {
    let mut checks = Vec::new();

    let governors: Vec<String> = entries(root, "/sys/devices/system/cpu", "cpu")
        .iter()
        .filter_map(|cpu| std::fs::read_to_string(cpu.join("cpufreq/scaling_governor")).ok())
        .map(|g| g.trim().to_string())
        .collect();
    if let Some(check) = governor_check(&governors) {
        checks.push(check);
    }

    let intel_no_turbo = read(root, "/sys/devices/system/cpu/intel_pstate/no_turbo");
    let boost = read(root, "/sys/devices/system/cpu/cpufreq/boost");
    if let Some(check) = turbo_check(intel_no_turbo.as_deref(), boost.as_deref()) {
        checks.push(check);
    }

    if let Some(load) = read(root, "/proc/loadavg").and_then(|l| parse_loadavg(&l)) {
        checks.push(load_check(load, cpus, LINUX_LOAD_PER_CPU));
    }

    let supplies: Vec<(String, String)> = entries(root, "/sys/class/power_supply", "")
        .iter()
        .filter_map(|supply| {
            let kind = std::fs::read_to_string(supply.join("type")).ok()?;
            let online = std::fs::read_to_string(supply.join("online")).unwrap_or_default();
            Some((kind.trim().to_string(), online.trim().to_string()))
        })
        .collect();
    if let Some(check) = power_check_linux(&supplies) {
        checks.push(check);
    }

    let temperatures: Vec<f64> = entries(root, "/sys/class/thermal", "thermal_zone")
        .iter()
        .filter_map(|zone| std::fs::read_to_string(zone.join("temp")).ok())
        .filter_map(|t| t.trim().parse::<f64>().ok())
        .map(|millidegrees| millidegrees / 1000.0)
        .collect();
    if let Some(check) = thermal_check(&temperatures) {
        checks.push(check);
    }

    checks
}

/// All CPUs should use the `performance` governor.
fn governor_check(governors: &[String]) -> Option<Check> {
    if governors.is_empty() {
        return None;
    }
    // e.g. "powersave on 6 CPUs, performance on 2 CPUs"
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for governor in governors {
        match counts.iter_mut().find(|(g, _)| g == governor) {
            Some((_, n)) => *n += 1,
            None => counts.push((governor, 1)),
        }
    }
    let detail = counts
        .iter()
        .map(|(g, n)| format!("{g} on {n} CPU{}", if *n == 1 { "" } else { "s" }))
        .collect::<Vec<_>>()
        .join(", ");
    let ok = governors.iter().all(|g| g == "performance");
    Some(
        Check::new(
            "CPU governor",
            if ok { Status::Ok } else { Status::Fail },
            detail,
        )
        .with_hint("sudo cpupower frequency-set -g performance"),
    )
}

/// Turbo boost makes the clock depend on temperature and load.
fn turbo_check(intel_no_turbo: Option<&str>, boost: Option<&str>) -> Option<Check> {
    let (enabled, hint) = match (intel_no_turbo, boost) {
        (Some(no_turbo), _) => (
            no_turbo == "0",
            "echo 1 | sudo tee /sys/devices/system/cpu/intel_pstate/no_turbo",
        ),
        (None, Some(boost)) => (
            boost == "1",
            "echo 0 | sudo tee /sys/devices/system/cpu/cpufreq/boost",
        ),
        (None, None) => return None,
    };
    Some(
        Check::new(
            "Turbo boost",
            if enabled { Status::Fail } else { Status::Ok },
            if enabled { "enabled" } else { "disabled" },
        )
        .with_hint(hint),
    )
}

/// First field of `/proc/loadavg`: the 1-minute load average.
fn parse_loadavg(line: &str) -> Option<f64> {
    line.split_whitespace().next()?.parse().ok()
}

/// Linux: the load average counts runnable (and uninterruptible) tasks, so a
/// quiet machine is close to 0.
const LINUX_LOAD_PER_CPU: f64 = 0.1;
/// macOS: the load average also counts many briefly waiting threads and sits
/// around 2–6 on an ordinary idle desktop (#188), so allow more.
#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
const MACOS_LOAD_PER_CPU: f64 = 0.5;

/// Other processes that keep CPUs busy compete with the benchmark.
fn load_check(load: f64, cpus: usize, per_cpu: f64) -> Check {
    let limit = (per_cpu * cpus as f64).max(1.0);
    Check::new(
        "Load average",
        if load < limit {
            Status::Ok
        } else {
            Status::Warn
        },
        format!("{load:.2} ({cpus} CPUs)"),
    )
    .with_hint("stop other programs, or benchmark on an idle machine")
}

/// `(type, online)` of each `/sys/class/power_supply` entry.
fn power_check_linux(supplies: &[(String, String)]) -> Option<Check> {
    let mains: Vec<&str> = supplies
        .iter()
        .filter(|(kind, _)| kind == "Mains")
        .map(|(_, online)| online.as_str())
        .collect();
    if mains.is_empty() {
        // Desktops and servers usually expose no power supply at all
        return None;
    }
    let on_battery = mains.iter().all(|&online| online == "0");
    Some(power_check(on_battery))
}

fn power_check(on_battery: bool) -> Check {
    Check::new(
        "Power source",
        if on_battery { Status::Warn } else { Status::Ok },
        if on_battery { "battery" } else { "AC" },
    )
    .with_hint("connect the power adapter (on battery, CPUs are often clocked down)")
}

/// Temperatures in °C: a hot CPU gets throttled.
fn thermal_check(temperatures: &[f64]) -> Option<Check> {
    let max = temperatures.iter().copied().fold(f64::NAN, f64::max);
    if max.is_nan() {
        return None;
    }
    Some(
        Check::new(
            "Thermal",
            if max < 85.0 { Status::Ok } else { Status::Warn },
            format!("max {max:.0} °C"),
        )
        .with_hint("let the machine cool down, or improve cooling"),
    )
}

// ---------------------------------------------------------------------------
// macOS

#[cfg(target_os = "macos")]
fn macos_checks(cpus: usize) -> Vec<Check> {
    let mut checks = Vec::new();

    let mut loads = [0.0f64; 3];
    // SAFETY: `loads` has room for the 3 values requested.
    if unsafe { libc::getloadavg(loads.as_mut_ptr(), 3) } >= 1 {
        checks.push(load_check(loads[0], cpus, MACOS_LOAD_PER_CPU));
    }

    let pmset = |args: &[&str]| {
        std::process::Command::new("pmset")
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
    };
    if let Some(on_battery) = pmset(&["-g", "batt"]).and_then(|out| parse_pmset_batt(&out)) {
        checks.push(power_check(on_battery));
    }
    if let Some(low_power) = pmset(&["-g"]).and_then(|out| parse_pmset_low_power(&out)) {
        checks.push(
            Check::new(
                "Low Power Mode",
                if low_power { Status::Warn } else { Status::Ok },
                if low_power { "on" } else { "off" },
            )
            .with_hint("turn off Low Power Mode in System Settings → Battery"),
        );
    }
    if let Some(limit) = pmset(&["-g", "therm"]).and_then(|out| parse_pmset_speed_limit(&out)) {
        checks.push(
            Check::new(
                "Thermal",
                if limit >= 100 {
                    Status::Ok
                } else {
                    Status::Warn
                },
                format!("CPU speed limit {limit}%"),
            )
            .with_hint("let the machine cool down, or improve cooling"),
        );
    }

    checks
}

/// `pmset -g batt`: "Now drawing from 'Battery Power'" / "'AC Power'".
#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
fn parse_pmset_batt(output: &str) -> Option<bool> {
    let line = output.lines().find(|l| l.contains("drawing from"))?;
    Some(line.contains("Battery Power"))
}

/// `pmset -g`: " lowpowermode         1"
#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
fn parse_pmset_low_power(output: &str) -> Option<bool> {
    let line = output
        .lines()
        .find(|l| l.split_whitespace().next() == Some("lowpowermode"))?;
    Some(line.split_whitespace().nth(1)? == "1")
}

/// `pmset -g therm`: "CPU_Speed_Limit \t= 100" (absent if never throttled).
#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
fn parse_pmset_speed_limit(output: &str) -> Option<u32> {
    let line = output.lines().find(|l| l.contains("CPU_Speed_Limit"))?;
    line.split('=').nth(1)?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn governor() {
        assert!(governor_check(&[]).is_none());
        let ok = governor_check(&strings(&["performance", "performance"])).unwrap();
        assert_eq!(ok.status, Status::Ok);
        assert_eq!(ok.detail, "performance on 2 CPUs");
        assert!(ok.hint.is_none());

        let mixed = governor_check(&strings(&["powersave", "performance", "powersave"])).unwrap();
        assert_eq!(mixed.status, Status::Fail);
        assert_eq!(mixed.detail, "powersave on 2 CPUs, performance on 1 CPU");
        assert!(mixed.hint.unwrap().contains("cpupower"));
    }

    #[test]
    fn turbo() {
        assert!(turbo_check(None, None).is_none());
        assert_eq!(turbo_check(Some("0"), None).unwrap().status, Status::Fail);
        assert_eq!(turbo_check(Some("1"), None).unwrap().status, Status::Ok);
        let amd = turbo_check(None, Some("1")).unwrap();
        assert_eq!(amd.status, Status::Fail);
        assert!(amd.hint.unwrap().contains("cpufreq/boost"));
        assert_eq!(turbo_check(None, Some("0")).unwrap().status, Status::Ok);
    }

    #[test]
    fn load() {
        assert_eq!(parse_loadavg("0.21 0.30 0.25 1/345 6789"), Some(0.21));
        assert_eq!(parse_loadavg(""), None);
        let linux = |load, cpus| load_check(load, cpus, LINUX_LOAD_PER_CPU).status;
        assert_eq!(linux(0.21, 8), Status::Ok);
        assert_eq!(linux(0.9, 2), Status::Ok); // below 1.0
        assert_eq!(linux(3.5, 8), Status::Warn);
        assert_eq!(linux(3.5, 64), Status::Ok); // below 6.4
        assert_eq!(
            load_check(0.21, 8, LINUX_LOAD_PER_CPU).detail,
            "0.21 (8 CPUs)"
        );

        // An idle macOS desktop (#188): 3.99 on 10 CPUs is fine, 6 is not
        let macos = |load, cpus| load_check(load, cpus, MACOS_LOAD_PER_CPU).status;
        assert_eq!(macos(3.99, 10), Status::Ok);
        assert_eq!(macos(6.0, 10), Status::Warn);
    }

    #[test]
    fn power() {
        let supply = |kind: &str, online: &str| (kind.to_string(), online.to_string());
        assert!(power_check_linux(&[]).is_none());
        assert!(power_check_linux(&[supply("Battery", "")]).is_none());
        let ac = power_check_linux(&[supply("Mains", "1"), supply("Battery", "")]).unwrap();
        assert_eq!((ac.status, ac.detail.as_str()), (Status::Ok, "AC"));
        let battery = power_check_linux(&[supply("Mains", "0")]).unwrap();
        assert_eq!(battery.status, Status::Warn);
    }

    #[test]
    fn thermal() {
        assert!(thermal_check(&[]).is_none());
        assert_eq!(thermal_check(&[45.0, 48.2]).unwrap().detail, "max 48 °C");
        assert_eq!(thermal_check(&[45.0, 91.0]).unwrap().status, Status::Warn);
    }

    #[test]
    fn pmset_parsing() {
        let batt = "Now drawing from 'Battery Power'\n -InternalBattery-0 (id=1)\t80%; discharging";
        assert_eq!(parse_pmset_batt(batt), Some(true));
        assert_eq!(
            parse_pmset_batt("Now drawing from 'AC Power'\n"),
            Some(false)
        );
        assert_eq!(parse_pmset_batt(""), None);

        let settings = "System-wide power settings:\n lowpowermode         1\n sleep 1\n";
        assert_eq!(parse_pmset_low_power(settings), Some(true));
        assert_eq!(parse_pmset_low_power(" lowpowermode 0\n"), Some(false));
        assert_eq!(parse_pmset_low_power(" sleep 1\n"), None);

        let therm =
            "CPU_Scheduler_Limit \t= 100\nCPU_Available_CPUs \t= 8\nCPU_Speed_Limit \t= 80\n";
        assert_eq!(parse_pmset_speed_limit(therm), Some(80));
        assert_eq!(
            parse_pmset_speed_limit("Note: No thermal warning level has been recorded\n"),
            None
        );
    }

    #[test]
    fn fake_linux_root() {
        let dir = tempfile::tempdir().unwrap();
        let write = |path: &str, content: &str| {
            let path = dir.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
        };
        write(
            "sys/devices/system/cpu/cpu0/cpufreq/scaling_governor",
            "powersave\n",
        );
        write(
            "sys/devices/system/cpu/cpu1/cpufreq/scaling_governor",
            "powersave\n",
        );
        write("sys/devices/system/cpu/intel_pstate/no_turbo", "1\n");
        write("proc/loadavg", "0.05 0.10 0.15 1/100 42\n");
        write("sys/class/power_supply/AC/type", "Mains\n");
        write("sys/class/power_supply/AC/online", "1\n");
        write("sys/class/thermal/thermal_zone0/temp", "47000\n");

        let checks = linux_checks(dir.path(), 8);
        let summary: Vec<(&str, Status, &str)> = checks
            .iter()
            .map(|c| (c.name, c.status, c.detail.as_str()))
            .collect();
        assert_eq!(
            summary,
            [
                ("CPU governor", Status::Fail, "powersave on 2 CPUs"),
                ("Turbo boost", Status::Ok, "disabled"),
                ("Load average", Status::Ok, "0.05 (8 CPUs)"),
                ("Power source", Status::Ok, "AC"),
                ("Thermal", Status::Ok, "max 47 °C"),
            ]
        );
    }
}
