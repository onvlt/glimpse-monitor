//! The data one sampling pass produces, plus the severity policy that turns
//! readings into chip and row colors.

use std::sync::Mutex;

/// How many samples the CPU history chart keeps.
pub const HISTORY_MAX: usize = 40;

/// Percentage where usage rows and chips turn warning.
pub const USAGE_WARN_PCT: f64 = 90.0;
/// Percentage where usage rows and chips turn error.
pub const USAGE_ERROR_PCT: f64 = 95.0;

/// Derived RAM reading.
#[derive(Clone, Debug, PartialEq)]
pub struct RamInfo {
    pub pct: f64,
    pub used: u64,
    pub total: u64,
}

/// One mount point's usage.
#[derive(Clone, Debug, PartialEq)]
pub struct DiskUsage {
    pub mount: String,
    pub used: u64,
    pub total: u64,
}

/// One temperature sensor, ready for display.
#[derive(Clone, Debug, PartialEq)]
pub struct TempReading {
    /// The sensor's own label, used as the row title.
    pub label: String,
    pub celsius: f32,
    /// The kernel's declared critical limit, when the sensor reports one.
    pub critical_c: Option<f32>,
    /// The highest reading sysinfo has seen since the applet started.
    pub peak_c: Option<f32>,
}

/// Everything one build pass renders from.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub cpu_pct: Option<f64>,
    pub ram: Option<RamInfo>,
    pub disks: Vec<DiskUsage>,
    pub temps: Vec<TempReading>,
    pub history: Vec<Option<f64>>,
}

/// What the sampling thread fills in and every build reads.
#[derive(Debug, Default)]
pub struct Shared {
    snapshot: Mutex<Snapshot>,
}

impl Shared {
    /// The readings to render right now.
    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }

    /// Publishes a new set of readings.
    pub fn publish(&self, snapshot: Snapshot) {
        *self.snapshot.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = snapshot;
    }
}

/// Pushes one CPU sample onto the history, evicting the oldest past 40.
pub fn push_history(history: &mut Vec<Option<f64>>, cpu_pct: Option<f64>) {
    if history.len() >= HISTORY_MAX {
        history.remove(0);
    }
    history.push(cpu_pct);
}

/// RAM reading derived from a total/available pair.
#[must_use]
pub fn ram_info(total: u64, available: u64) -> Option<RamInfo> {
    if total == 0 {
        return None;
    }
    let used = total.saturating_sub(available.min(total));
    Some(RamInfo {
        used,
        total,
        pct: crate::format::usage_pct(used, total),
    })
}

/// Warning/error coloring for a percent reading.
#[must_use]
pub fn usage_tone(pct: f64) -> Tone {
    if pct >= USAGE_ERROR_PCT {
        Tone::Error
    } else if pct >= USAGE_WARN_PCT {
        Tone::Warning
    } else {
        Tone::None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    None,
    Warning,
    Error,
}

/// Warning/error coloring for a temperature reading.
///
/// Sensors declaring their own critical limit get thresholds scaled off it;
/// the rest use the configured 75/85 °C fallback.
#[must_use]
pub fn temp_tone(reading: &TempReading, warn_c: f32, error_c: f32) -> Tone {
    let (warn, error) = match reading.critical_c {
        Some(critical) => (critical - 15.0, critical - 2.0),
        None => (warn_c, error_c),
    };
    if reading.celsius >= error {
        Tone::Error
    } else if reading.celsius >= warn {
        Tone::Warning
    } else {
        Tone::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_keeps_at_most_40_samples() {
        let mut history = Vec::new();
        for index in 0..45 {
            push_history(&mut history, Some(f64::from(index)));
        }
        assert_eq!(history.len(), HISTORY_MAX);
        assert_eq!(history[0], Some(5.0));
        assert_eq!(*history.last().expect("last"), Some(44.0));

        let mut history = vec![Some(1.0)];
        push_history(&mut history, None);
        assert_eq!(history, vec![Some(1.0), None]);
    }

    #[test]
    fn ram_pct_clamps_available() {
        let info = ram_info(1000, 250).expect("info");
        assert_eq!(info.used, 750);
        assert!((info.pct - 75.0).abs() < f64::EPSILON);
        assert_eq!(ram_info(0, 0), None);
        assert_eq!(ram_info(1000, 1500).expect("clamped").used, 0);
    }

    #[test]
    fn temperature_tone_scales_with_a_declared_critical_limit() {
        let reading = |c: f32, critical: Option<f32>| TempReading {
            label: "test".to_owned(),
            celsius: c,
            critical_c: critical,
            peak_c: None,
        };
        // critical 95 °C -> warning at 80, error at 93
        assert_eq!(temp_tone(&reading(79.0, Some(95.0)), 75.0, 85.0), Tone::None);
        assert_eq!(temp_tone(&reading(80.0, Some(95.0)), 75.0, 85.0), Tone::Warning);
        assert_eq!(temp_tone(&reading(92.9, Some(95.0)), 75.0, 85.0), Tone::Warning);
        assert_eq!(temp_tone(&reading(93.0, Some(95.0)), 75.0, 85.0), Tone::Error);
        // no declared limit -> the configured thresholds
        assert_eq!(temp_tone(&reading(74.9, None), 75.0, 85.0), Tone::None);
        assert_eq!(temp_tone(&reading(75.0, None), 75.0, 85.0), Tone::Warning);
        assert_eq!(temp_tone(&reading(85.0, None), 75.0, 85.0), Tone::Error);
    }

    #[test]
    fn usage_tone_matches_thresholds() {
        assert_eq!(usage_tone(89.9), Tone::None);
        assert_eq!(usage_tone(90.0), Tone::Warning);
        assert_eq!(usage_tone(95.0), Tone::Error);
    }

    #[test]
    fn snapshot_publish_and_read_round_trips() {
        let shared = Shared::default();
        assert_eq!(shared.snapshot(), Snapshot::default());
        shared.publish(Snapshot {
            cpu_pct: Some(12.0),
            history: vec![Some(12.0)],
            ..Snapshot::default()
        });
        assert_eq!(shared.snapshot().cpu_pct, Some(12.0));
    }
}
