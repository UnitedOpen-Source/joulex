use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Target metric used as the primary objective for benchmarking, comparison, and sorting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    #[default]
    Wall,
    Cpu,
    User,
    System,
    Energy,
    Memory,
}

impl Metric {
    /// Canonical lowercase name of the metric
    pub fn name(&self) -> &'static str {
        match self {
            Metric::Wall => "wall",
            Metric::Cpu => "cpu",
            Metric::User => "user",
            Metric::System => "system",
            Metric::Energy => "energy",
            Metric::Memory => "memory",
        }
    }

    /// Header name displayed in the terminal output
    pub fn display_header(&self) -> &'static str {
        match self {
            Metric::Wall => "Time",
            Metric::Cpu => "CPU Time",
            Metric::User => "User Time",
            Metric::System => "System Time",
            Metric::Energy => "Energy",
            Metric::Memory => "Memory",
        }
    }

    /// Verb used in summary sentences, e.g. "command ran" vs "command used"
    pub fn verb(&self) -> &'static str {
        match self {
            Metric::Wall | Metric::Cpu | Metric::User | Metric::System => "ran",
            Metric::Energy | Metric::Memory => "used",
        }
    }

    /// Comparison words: (better, worse), e.g. ("faster", "slower") or ("less energy", "more energy")
    pub fn comparison_words(&self) -> (&'static str, &'static str) {
        match self {
            Metric::Wall | Metric::Cpu | Metric::User | Metric::System => ("faster", "slower"),
            Metric::Energy => ("less energy", "more energy"),
            Metric::Memory => ("less memory", "more memory"),
        }
    }

    /// Phrase used when two commands have equal measurements
    pub fn equal_word(&self) -> &'static str {
        match self {
            Metric::Wall | Metric::Cpu | Metric::User | Metric::System => "as fast as",
            Metric::Energy => "same energy as",
            Metric::Memory => "same memory as",
        }
    }

    /// Whether this metric represents a duration in time
    pub fn is_time(&self) -> bool {
        matches!(
            self,
            Metric::Wall | Metric::Cpu | Metric::User | Metric::System
        )
    }
}

impl FromStr for Metric {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "wall" | "wall-clock" | "time" => Ok(Metric::Wall),
            "cpu" | "total-cpu" => Ok(Metric::Cpu),
            "user" => Ok(Metric::User),
            "system" => Ok(Metric::System),
            "energy" => Ok(Metric::Energy),
            "memory" | "rss" | "peak-memory" => Ok(Metric::Memory),
            other => Err(format!(
                "invalid metric '{other}': possible values are wall, cpu, user, system, energy, memory"
            )),
        }
    }
}

impl fmt::Display for Metric {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metric_defaults_and_names() {
        assert_eq!(Metric::default(), Metric::Wall);
        assert_eq!(Metric::Wall.name(), "wall");
        assert_eq!(Metric::Cpu.name(), "cpu");
        assert_eq!(Metric::User.name(), "user");
        assert_eq!(Metric::System.name(), "system");
        assert_eq!(Metric::Energy.name(), "energy");
        assert_eq!(Metric::Memory.name(), "memory");
    }

    #[test]
    fn test_metric_verbs_and_comparisons() {
        assert_eq!(Metric::Wall.verb(), "ran");
        assert_eq!(Metric::Energy.verb(), "used");
        assert_eq!(Metric::Memory.verb(), "used");

        assert_eq!(Metric::Wall.comparison_words(), ("faster", "slower"));
        assert_eq!(
            Metric::Energy.comparison_words(),
            ("less energy", "more energy")
        );
        assert_eq!(
            Metric::Memory.comparison_words(),
            ("less memory", "more memory")
        );
    }

    #[test]
    fn test_metric_is_time() {
        assert!(Metric::Wall.is_time());
        assert!(Metric::Cpu.is_time());
        assert!(Metric::User.is_time());
        assert!(Metric::System.is_time());
        assert!(!Metric::Energy.is_time());
        assert!(!Metric::Memory.is_time());
    }

    #[test]
    fn test_metric_from_str() {
        assert_eq!("wall".parse::<Metric>().unwrap(), Metric::Wall);
        assert_eq!("wall-clock".parse::<Metric>().unwrap(), Metric::Wall);
        assert_eq!("time".parse::<Metric>().unwrap(), Metric::Wall);
        assert_eq!("cpu".parse::<Metric>().unwrap(), Metric::Cpu);
        assert_eq!("total-cpu".parse::<Metric>().unwrap(), Metric::Cpu);
        assert_eq!("user".parse::<Metric>().unwrap(), Metric::User);
        assert_eq!("system".parse::<Metric>().unwrap(), Metric::System);
        assert_eq!("energy".parse::<Metric>().unwrap(), Metric::Energy);
        assert_eq!("memory".parse::<Metric>().unwrap(), Metric::Memory);
        assert_eq!("rss".parse::<Metric>().unwrap(), Metric::Memory);
        assert_eq!("peak-memory".parse::<Metric>().unwrap(), Metric::Memory);
        assert!("invalid".parse::<Metric>().is_err());
    }
}
