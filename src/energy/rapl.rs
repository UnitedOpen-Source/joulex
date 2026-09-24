use std::fs;
use std::path::{Path, PathBuf};

use super::EnergySampler;

/// Linux Intel/AMD RAPL (Running Average Power Limit) Sampler via sysfs powercap interface.
/// Path: /sys/class/powercap/intel-rapl/intel-rapl:0/energy_uj
pub struct LinuxRaplSampler {
    energy_files: Vec<PathBuf>,
    max_energy_ranges: Vec<u64>,
    start_readings: Vec<u64>,
}

impl LinuxRaplSampler {
    pub fn try_new() -> Option<Self> {
        let base = Path::new("/sys/class/powercap/intel-rapl");
        if !base.exists() {
            return None;
        }

        let mut energy_files = Vec::new();
        let mut max_energy_ranges = Vec::new();

        // Check top-level package domains: intel-rapl:0, intel-rapl:1, etc.
        let entries = fs::read_dir(base).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = path.file_name()?.to_string_lossy();
            if file_name.starts_with("intel-rapl:") && !file_name.contains(':') == false {
                let energy_uj_path = path.join("energy_uj");
                let max_range_path = path.join("max_energy_range_uj");

                if energy_uj_path.exists() {
                    // Try to read it to check permissions
                    if let Ok(content) = fs::read_to_string(&energy_uj_path) {
                        if content.trim().parse::<u64>().is_ok() {
                            let max_range = fs::read_to_string(&max_range_path)
                                .ok()
                                .and_then(|s| s.trim().parse::<u64>().ok())
                                .unwrap_or(u64::MAX);

                            energy_files.push(energy_uj_path);
                            max_energy_ranges.push(max_range);
                        }
                    }
                }
            }
        }

        if energy_files.is_empty() {
            return None;
        }

        Some(Self {
            energy_files,
            max_energy_ranges,
            start_readings: Vec::new(),
        })
    }

    fn read_all_uj(&self) -> Vec<u64> {
        self.energy_files
            .iter()
            .map(|path| {
                fs::read_to_string(path)
                    .ok()
                    .and_then(|s| s.trim().parse::<u64>().ok())
                    .unwrap_or(0)
            })
            .collect()
    }
}

impl EnergySampler for LinuxRaplSampler {
    fn start(&mut self) {
        self.start_readings = self.read_all_uj();
    }

    fn stop(&mut self) -> Option<f64> {
        if self.start_readings.is_empty() {
            return None;
        }

        let stop_readings = self.read_all_uj();
        let mut total_microjoules: f64 = 0.0;

        for i in 0..self.energy_files.len() {
            let start = self.start_readings.get(i).copied().unwrap_or(0);
            let stop = stop_readings.get(i).copied().unwrap_or(0);
            let max_range = self.max_energy_ranges.get(i).copied().unwrap_or(u64::MAX);

            let diff = if stop >= start {
                stop - start
            } else {
                // Counter wrapped around
                (max_range.saturating_sub(start)).saturating_add(stop)
            };

            total_microjoules += diff as f64;
        }

        // Convert microjoules to Joules (1 J = 1,000,000 uJ)
        Some(total_microjoules / 1_000_000.0)
    }

    fn is_available(&self) -> bool {
        !self.energy_files.is_empty()
    }
}
