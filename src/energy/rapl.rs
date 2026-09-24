use std::fs;
use std::path::{Path, PathBuf};

use super::EnergySampler;

/// Linux Intel/AMD RAPL (Running Average Power Limit) Sampler via sysfs powercap interface.
/// Path: /sys/class/powercap/intel-rapl/intel-rapl:0/energy_uj
pub struct LinuxRaplSampler {
    energy_files: Vec<PathBuf>,
    /// `max_energy_range_uj` per domain (None if unreadable)
    max_energy_ranges: Vec<Option<u64>>,
    /// Readings taken by `start()`, or None if any counter could not be read
    start_readings: Option<Vec<u64>>,
}

struct RaplDomain {
    name: String,
    energy_file: PathBuf,
    max_range: Option<u64>,
}

impl LinuxRaplSampler {
    pub fn try_new() -> Option<Self> {
        Self::try_new_at(Path::new("/sys/class/powercap/intel-rapl"))
    }

    pub fn try_new_at(base: &Path) -> Option<Self> {
        if !base.exists() {
            return None;
        }

        let mut domains = Vec::new();

        // Check top-level package domains: intel-rapl:0, intel-rapl:1, etc.
        let entries = fs::read_dir(base).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = path.file_name()?.to_string_lossy();
            if file_name.starts_with("intel-rapl:") && file_name.matches(':').count() == 1 {
                let energy_uj_path = path.join("energy_uj");
                let max_range_path = path.join("max_energy_range_uj");
                let name_path = path.join("name");

                if energy_uj_path.exists() {
                    // Try to read it to check permissions
                    if let Ok(content) = fs::read_to_string(&energy_uj_path) {
                        if content.trim().parse::<u64>().is_ok() {
                            let max_range = fs::read_to_string(&max_range_path)
                                .ok()
                                .and_then(|s| s.trim().parse::<u64>().ok());

                            let name = fs::read_to_string(&name_path)
                                .map(|s| s.trim().to_string())
                                .unwrap_or_default();

                            domains.push(RaplDomain {
                                name,
                                energy_file: energy_uj_path,
                                max_range,
                            });
                        }
                    }
                }
            }
        }

        if domains.is_empty() {
            return None;
        }

        // Domain selection strategy:
        // 1. If any domain name starts with "package-", sum only package-* domains.
        //    This avoids double-counting platform energy (psys) which overlaps with package energy.
        // 2. If no package-* domain is readable but "psys" is, use "psys" alone.
        // 3. Otherwise, use all available top-level domains.
        let has_packages = domains.iter().any(|d| d.name.starts_with("package-"));
        let selected: Vec<RaplDomain> = if has_packages {
            domains
                .into_iter()
                .filter(|d| d.name.starts_with("package-"))
                .collect()
        } else if domains.iter().any(|d| d.name == "psys") {
            domains.into_iter().filter(|d| d.name == "psys").collect()
        } else {
            domains
        };

        let mut energy_files = Vec::with_capacity(selected.len());
        let mut max_energy_ranges = Vec::with_capacity(selected.len());

        for domain in selected {
            energy_files.push(domain.energy_file);
            max_energy_ranges.push(domain.max_range);
        }

        Some(Self {
            energy_files,
            max_energy_ranges,
            start_readings: None,
        })
    }

    /// Read all energy counters, or None if any of them can't be read. A
    /// failed read must not be mistaken for a reading of 0, which would look
    /// like a counter wrap-around and produce a huge bogus energy value.
    fn read_all_uj(&self) -> Option<Vec<u64>> {
        self.energy_files
            .iter()
            .map(|path| {
                fs::read_to_string(path)
                    .ok()
                    .and_then(|s| s.trim().parse::<u64>().ok())
            })
            .collect()
    }
}

/// Energy consumed between two readings of a counter that wraps around at
/// `max_range` (the largest value it can hold, `max_energy_range_uj`).
/// Returns None for a wrap-around when the range is unknown.
fn counter_delta(start: u64, stop: u64, max_range: Option<u64>) -> Option<u64> {
    if stop >= start {
        Some(stop - start)
    } else {
        // The counter went from `start` up to `max_range`, wrapped to 0 and
        // then went up to `stop`: (max_range - start) + 1 + stop.
        let max_range = max_range?;
        Some(
            max_range
                .checked_sub(start)?
                .saturating_add(1)
                .saturating_add(stop),
        )
    }
}

impl EnergySampler for LinuxRaplSampler {
    fn start(&mut self) {
        self.start_readings = self.read_all_uj();
    }

    fn stop(&mut self) -> Option<f64> {
        // No sample if either the start or the stop reading failed, rather
        // than reporting a wrong value.
        let start_readings = self.start_readings.take()?;
        let stop_readings = self.read_all_uj()?;
        let mut total_microjoules: f64 = 0.0;

        for ((&start, &stop), &max_range) in start_readings
            .iter()
            .zip(&stop_readings)
            .zip(&self.max_energy_ranges)
        {
            total_microjoules += counter_delta(start, stop, max_range)? as f64;
        }

        // Convert microjoules to Joules (1 J = 1,000,000 uJ)
        Some(total_microjoules / 1_000_000.0)
    }

    fn is_available(&self) -> bool {
        !self.energy_files.is_empty()
    }
}

impl LinuxRaplSampler {
    #[cfg(test)]
    pub fn energy_files(&self) -> &[PathBuf] {
        &self.energy_files
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_fake_rapl_domain(
        base: &Path,
        dir_name: &str,
        domain_name: &str,
        energy_uj: u64,
        max_range_uj: Option<u64>,
    ) -> PathBuf {
        let domain_dir = base.join(dir_name);
        fs::create_dir_all(&domain_dir).unwrap();
        fs::write(domain_dir.join("name"), format!("{domain_name}\n")).unwrap();
        fs::write(domain_dir.join("energy_uj"), format!("{energy_uj}\n")).unwrap();
        if let Some(max_range) = max_range_uj {
            fs::write(
                domain_dir.join("max_energy_range_uj"),
                format!("{max_range}\n"),
            )
            .unwrap();
        }
        domain_dir
    }

    #[test]
    fn test_rapl_skips_psys_when_package_present() {
        let dir = tempdir().unwrap();
        let base = dir.path();

        let pkg0 = create_fake_rapl_domain(base, "intel-rapl:0", "package-0", 1_000_000, None);
        let _psys = create_fake_rapl_domain(base, "intel-rapl:1", "psys", 2_500_000, None);

        let mut sampler = LinuxRaplSampler::try_new_at(base).expect("sampler should be created");
        assert_eq!(sampler.energy_files().len(), 1);
        assert_eq!(sampler.energy_files()[0], pkg0.join("energy_uj"));

        sampler.start();
        fs::write(pkg0.join("energy_uj"), "2000000\n").unwrap();
        // psys changed too, but should be ignored
        fs::write(_psys.join("energy_uj"), "5000000\n").unwrap();

        let energy = sampler.stop().expect("should return energy");
        assert!((energy - 1.0).abs() < 1e-6); // 1,000,000 uJ = 1.0 J
    }

    #[test]
    fn test_rapl_sums_multiple_packages() {
        let dir = tempdir().unwrap();
        let base = dir.path();

        let pkg0 = create_fake_rapl_domain(base, "intel-rapl:0", "package-0", 1_000_000, None);
        let pkg1 = create_fake_rapl_domain(base, "intel-rapl:1", "package-1", 2_000_000, None);
        let _psys = create_fake_rapl_domain(base, "intel-rapl:2", "psys", 10_000_000, None);

        let mut sampler = LinuxRaplSampler::try_new_at(base).expect("sampler should be created");
        assert_eq!(sampler.energy_files().len(), 2);

        sampler.start();
        fs::write(pkg0.join("energy_uj"), "2000000\n").unwrap(); // +1.0 J
        fs::write(pkg1.join("energy_uj"), "3500000\n").unwrap(); // +1.5 J

        let energy = sampler.stop().expect("should return energy");
        assert!((energy - 2.5).abs() < 1e-6); // 1.0 + 1.5 = 2.5 J
    }

    #[test]
    fn test_rapl_uses_psys_when_no_packages() {
        let dir = tempdir().unwrap();
        let base = dir.path();

        let psys = create_fake_rapl_domain(base, "intel-rapl:0", "psys", 1_000_000, None);

        let mut sampler = LinuxRaplSampler::try_new_at(base).expect("sampler should be created");
        assert_eq!(sampler.energy_files().len(), 1);
        assert_eq!(sampler.energy_files()[0], psys.join("energy_uj"));

        sampler.start();
        fs::write(psys.join("energy_uj"), "2500000\n").unwrap();

        let energy = sampler.stop().expect("should return energy");
        assert!((energy - 1.5).abs() < 1e-6); // 1.5 J
    }

    #[test]
    fn test_rapl_counter_wraparound() {
        let dir = tempdir().unwrap();
        let base = dir.path();

        let pkg0 =
            create_fake_rapl_domain(base, "intel-rapl:0", "package-0", 900_000, Some(1_000_000));

        let mut sampler = LinuxRaplSampler::try_new_at(base).expect("sampler should be created");
        sampler.start();
        // Wraps around past max range 1_000_000 to 200_000:
        // diff = (1_000_000 - 900_000) + 1 + 200_000 = 300_001 uJ
        fs::write(pkg0.join("energy_uj"), "200000\n").unwrap();

        let energy = sampler.stop().expect("should return energy");
        assert!((energy - 0.300_001).abs() < 1e-9);
    }

    #[test]
    fn test_rapl_read_failure_yields_no_sample() {
        let dir = tempdir().unwrap();
        let base = dir.path();

        let pkg0 =
            create_fake_rapl_domain(base, "intel-rapl:0", "package-0", 900_000, Some(1_000_000));

        let mut sampler = LinuxRaplSampler::try_new_at(base).expect("sampler should be created");
        sampler.start();
        // An unreadable/garbled counter used to be read as 0, which looked like
        // a wrap-around and reported ~0.1 J of bogus energy.
        fs::write(pkg0.join("energy_uj"), "garbage\n").unwrap();
        assert_eq!(sampler.stop(), None);

        // A failed start reading also yields no sample
        sampler.start();
        fs::write(pkg0.join("energy_uj"), "950000\n").unwrap();
        assert_eq!(sampler.stop(), None);

        // …and the sampler recovers once the counter is readable again
        sampler.start();
        fs::write(pkg0.join("energy_uj"), "960000\n").unwrap();
        let energy = sampler.stop().expect("should return energy");
        assert!((energy - 0.01).abs() < 1e-9);
    }

    #[test]
    fn test_rapl_wraparound_with_unknown_range_yields_no_sample() {
        let dir = tempdir().unwrap();
        let base = dir.path();

        let pkg0 = create_fake_rapl_domain(base, "intel-rapl:0", "package-0", 900_000, None);

        let mut sampler = LinuxRaplSampler::try_new_at(base).expect("sampler should be created");
        sampler.start();
        fs::write(pkg0.join("energy_uj"), "200000\n").unwrap();
        assert_eq!(sampler.stop(), None);
    }

    #[test]
    fn test_counter_delta() {
        assert_eq!(counter_delta(10, 25, None), Some(15));
        assert_eq!(counter_delta(10, 25, Some(100)), Some(15));
        assert_eq!(counter_delta(90, 5, Some(100)), Some(16)); // 90..=100 → 0..=5
        assert_eq!(counter_delta(90, 5, None), None);
        assert_eq!(counter_delta(150, 5, Some(100)), None); // start beyond range
    }
}
