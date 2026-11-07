//! The data one sampling pass produces, plus the severity policy that turns
//! readings into chip and row colors.

use std::sync::Mutex;

/// How many samples the CPU history chart keeps.
pub const HISTORY_MAX: usize = 40;

/// How many interfaces the `net` chip and Network section show.
pub const NET_VISIBLE_MAX: usize = 4;

/// Percentage where usage rows and chips turn warning.
pub const USAGE_WARN_PCT: f64 = 90.0;
/// Percentage where usage rows and chips turn error.
pub const USAGE_ERROR_PCT: f64 = 95.0;

/// A used/total share of some capacity (RAM or swap).
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

/// One interface's traffic, as seen by the last sampling tick.
#[derive(Clone, Debug, PartialEq)]
pub struct NetSample {
    pub name: String,
    /// Bytes per second received, measured over the real tick length.
    pub down_speed: f64,
    pub up_speed: f64,
    pub down_total: u64,
    pub up_total: u64,
}

impl NetSample {
    fn traffic(&self) -> f64 {
        self.down_speed + self.up_speed
    }
}

/// Raw counters for one interface between two refreshes.
pub struct RawIface {
    pub name: String,
    pub down_delta: u64,
    pub up_delta: u64,
    pub down_total: u64,
    pub up_total: u64,
}

/// One host-bound interface; empty names and loopback-like names are skipped.
fn is_visible(name: &str) -> bool {
    !name.is_empty() && name != "lo" && !name.starts_with("lo.")
}

/// Turns raw interface counters into visible `net` rows:
/// loopback skipped, busiest interface first, capped to four.
#[must_use]
pub fn net_samples(ifaces: &[RawIface], elapsed_seconds: f64) -> Vec<NetSample> {
    let elapsed = if elapsed_seconds.is_finite() && elapsed_seconds > 0.001 {
        elapsed_seconds
    } else {
        0.001
    };
    let mut samples: Vec<NetSample> = ifaces
        .iter()
        .filter(|iface| is_visible(&iface.name))
        .map(|iface| NetSample {
            name: iface.name.clone(),
            down_speed: iface.down_delta as f64 / elapsed,
            up_speed: iface.up_delta as f64 / elapsed,
            down_total: iface.down_total,
            up_total: iface.up_total,
        })
        .collect();
    samples.sort_by(|a, b| {
        b.traffic()
            .partial_cmp(&a.traffic())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    samples.truncate(NET_VISIBLE_MAX);
    samples
}

/// Load average over 1/5/15 minutes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoadInfo {
    pub one: f64,
    pub five: f64,
    pub fifteen: f64,
}

/// Static host facts, collected once when the sampling thread starts.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SystemInfo {
    pub kernel: Option<String>,
    pub os: Option<String>,
    pub hostname: Option<String>,
}

/// One adapter node of the system.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GpuReading {
    pub model: String,
    pub usage: Option<f64>,
    pub memory_used: Option<u64>,
    pub memory_total: Option<u64>,
}

/// Everything one build pass renders from.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub cpu_pct: Option<f64>,
    pub ram: Option<RamInfo>,
    pub swap: Option<RamInfo>,
    pub disks: Vec<DiskUsage>,
    pub temps: Vec<TempReading>,
    pub net: Vec<NetSample>,
    pub load: Option<LoadInfo>,
    pub uptime_seconds: Option<u64>,
    pub cores: Vec<f64>,
    pub gpus: Vec<GpuReading>,
    pub system_info: Option<SystemInfo>,
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
        self.snapshot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Publishes a new set of readings.
    pub fn publish(&self, snapshot: Snapshot) {
        *self
            .snapshot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = snapshot;
    }
}

/// Pushes one CPU sample onto the history, evicting the oldest past 40.
pub fn push_history(history: &mut Vec<Option<f64>>, cpu_pct: Option<f64>) {
    if history.len() >= HISTORY_MAX {
        history.remove(0);
    }
    history.push(cpu_pct);
}

/// A used/total share derived from a total/available pair.
#[must_use]
pub fn usage_info(total: u64, available: u64) -> Option<RamInfo> {
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
        let info = usage_info(1000, 250).expect("info");
        assert_eq!(info.used, 750);
        assert!((info.pct - 75.0).abs() < f64::EPSILON);
        assert_eq!(usage_info(0, 0), None);
        assert_eq!(usage_info(1000, 1500).expect("clamped").used, 0);
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
    fn net_samples_skip_loopback_rank_and_cap() {
        let ifaces = |of: &[(u64, u64)]| {
            of.iter()
                .map(|(down, up)| RawIface {
                    name: format!("i{}", down),
                    down_delta: *down,
                    up_delta: *up,
                    down_total: *down,
                    up_total: *up,
                })
                .collect::<Vec<_>>()
        };

        let samples = net_samples(
            &ifaces(&[
                (1_000, 0),
                (0, 0),
                (300, 4_300),
                (500, 900),
            ]),
            2.0,
        );
        let names: Vec<&str> = samples.iter().map(|sample| sample.name.as_str()).collect();
        // ranked by down+up speed: 2300, 700, 500, 0
        assert_eq!(names, ["i300", "i500", "i1000", "i0"]);
        assert!((samples[0].up_speed - 2_150.0).abs() < 0.001);

        let idle = net_samples(&ifaces(&[(0, 0)]), 2.0);
        assert_eq!(idle.len(), 1);
        assert_eq!(idle[0].down_speed, 0.0);

        let tiny = net_samples(&ifaces(&[]), 0.0);
        assert!(tiny.is_empty());

        let loopback = net_samples(&[RawIface {
            name: "lo".to_owned(),
            down_delta: 1,
            up_delta: 1,
            down_total: 1,
            up_total: 1,
        }], 1.0);
        assert!(loopback.is_empty());
    }

    #[test]
    fn net_samples_survive_a_zero_tick() {
        let samples = net_samples(
            &[RawIface {
                name: "eth0".to_owned(),
                down_delta: 100,
                up_delta: 0,
                down_total: 100,
                up_total: 0,
            }],
            0.0,
        );
        assert!((samples[0].down_speed - 100.0) < f64::MAX);
        assert!(samples[0].down_speed.is_finite());
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
