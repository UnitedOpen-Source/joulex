pub mod rapl;

pub trait EnergySampler: Send + Sync {
    /// Start an energy measurement window
    fn start(&mut self);

    /// Stop measurement and return energy consumed in Joules during window
    fn stop(&mut self) -> Option<f64>;

    /// Check if energy measurement is supported and available on this platform
    fn is_available(&self) -> bool;
}

pub fn get_energy_sampler() -> Box<dyn EnergySampler> {
    #[cfg(target_os = "linux")]
    {
        if let Some(sampler) = rapl::LinuxRaplSampler::try_new() {
            return Box::new(sampler);
        }
    }
    Box::new(DummySampler::new())
}

pub struct DummySampler {
    start_time: Option<std::time::Instant>,
}

impl DummySampler {
    pub fn new() -> Self {
        Self { start_time: None }
    }
}

impl Default for DummySampler {
    fn default() -> Self {
        Self::new()
    }
}

impl EnergySampler for DummySampler {
    fn start(&mut self) {
        self.start_time = Some(std::time::Instant::now());
    }

    fn stop(&mut self) -> Option<f64> {
        // Dummy/fallback sampler does not provide hardware energy measurements
        None
    }

    fn is_available(&self) -> bool {
        false
    }
}

pub struct MockEnergySampler {
    active: bool,
}

impl MockEnergySampler {
    pub fn new() -> Self {
        Self { active: false }
    }
}

impl Default for MockEnergySampler {
    fn default() -> Self {
        Self::new()
    }
}

impl EnergySampler for MockEnergySampler {
    fn start(&mut self) {
        self.active = true;
    }

    fn stop(&mut self) -> Option<f64> {
        if self.active {
            self.active = false;
            Some(1.25)
        } else {
            None
        }
    }

    fn is_available(&self) -> bool {
        true
    }
}
