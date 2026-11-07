//! Panel config parsing. Options arrive as a JSON object validated here, not
//! trusted; every violation becomes a message the panel shows in a chip tooltip.
//!
//! The public option key is `widgets` (compatibility with the panel config
//! format); everything internal talks about chips.

use serde_json::{Map, Value};
use std::collections::HashSet;

/// One configurable bar indicator bound to a metric.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Chip {
    Cpu,
    Ram,
    Swap,
    Disk,
    Temp,
    Net,
    Load,
    Uptime,
}

impl Chip {
    fn from_name(value: &str) -> Option<Self> {
        match value {
            "cpu" => Some(Self::Cpu),
            "ram" => Some(Self::Ram),
            "swap" => Some(Self::Swap),
            "disk" => Some(Self::Disk),
            "temp" => Some(Self::Temp),
            "net" => Some(Self::Net),
            "load" => Some(Self::Load),
            "uptime" => Some(Self::Uptime),
            _ => None,
        }
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Ram => "ram",
            Self::Swap => "swap",
            Self::Disk => "disk",
            Self::Temp => "temp",
            Self::Net => "net",
            Self::Load => "load",
            Self::Uptime => "uptime",
        }
    }
}

/// One applet instance's validated settings.
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    /// Seconds between samples.
    pub interval_seconds: u64,
    /// Chips to draw on the panel, in the configured order.
    pub chips: Vec<Chip>,
    /// Mount points shown in the storage section, in the configured order.
    pub mounts: Vec<String>,
    /// RAM chip and row style.
    pub ram_display: DisplayMode,
    /// Swap chip and row style.
    pub swap_display: DisplayMode,
    /// Disk chip and row style.
    pub disk_display: DisplayMode,
    /// Fallback warn threshold when a sensor has no critical limit.
    pub temp_warn_c: f32,
    /// Fallback error threshold when a sensor has no critical limit.
    pub temp_error_c: f32,
    /// Temperature scale used by the chip and rows.
    pub temp_unit: TempUnit,
}

use crate::format::{DisplayMode, TempUnit};

impl Default for Config {
    fn default() -> Self {
        Self {
            interval_seconds: 2,
            chips: vec![Chip::Cpu, Chip::Ram, Chip::Disk, Chip::Temp],
            mounts: vec!["/".to_owned()],
            ram_display: DisplayMode::Percent,
            swap_display: DisplayMode::Percent,
            disk_display: DisplayMode::Percent,
            temp_warn_c: 75.0,
            temp_error_c: 85.0,
            temp_unit: TempUnit::Celsius,
        }
    }
}

/// Parses the options object, reporting every problem at once in
/// `"<path>: <message>"` form.
pub fn parse(options: &Value) -> Result<Config, Vec<String>> {
    let mut issues = Vec::new();
    let object = match options {
        Value::Null => Map::new(),
        Value::Object(map) => map.clone(),
        _ => {
            return Err(vec!["settings: must be a settings object".to_owned()]);
        }
    };
    let object = &object;

    let interval_seconds = parse_interval(object, &mut issues);
    let chips = parse_chips(object, &mut issues);
    let mounts = parse_mounts(object, &mut issues);
    let ram_display = parse_display_mode(object, "ram-display", &mut issues);
    let swap_display = parse_display_mode(object, "swap-display", &mut issues);
    let disk_display = parse_display_mode(object, "disk-display", &mut issues);
    let (temp_warn_c, temp_error_c) = parse_thresholds(object, &mut issues);
    let temp_unit = parse_temp_unit(object, &mut issues);

    if issues.is_empty() {
        Ok(Config {
            interval_seconds,
            chips,
            mounts,
            ram_display,
            swap_display,
            disk_display,
            temp_warn_c,
            temp_error_c,
            temp_unit,
        })
    } else {
        Err(issues)
    }
}

fn parse_interval(object: &Map<String, Value>, issues: &mut Vec<String>) -> u64 {
    let key = "interval";
    let Some(value) = optional_number(object, key, issues) else {
        return 2;
    };
    if value.fract() != 0.0 {
        issues.push(format!("{key}: must be an integer"));
    } else if value < 1.0 {
        issues.push(format!("{key}: must be at least 1"));
    } else if value > 60.0 {
        issues.push(format!("{key}: must be at most 60"));
    } else {
        return value as u64;
    }
    2
}

fn parse_chips(object: &Map<String, Value>, issues: &mut Vec<String>) -> Vec<Chip> {
    let key = "widgets";
    let Some(value) = object.get(key).filter(|value| !value.is_null()) else {
        return Config::default().chips;
    };
    let Some(items) = value.as_array() else {
        issues.push(format!("{key}: must be a list of widget names"));
        return Config::default().chips;
    };
    if items.is_empty() {
        issues.push(format!("{key}: must include at least one widget"));
        return Config::default().chips;
    }

    let chips: Vec<Option<Chip>> = items
        .iter()
        .map(|item| item.as_str().and_then(Chip::from_name))
        .collect();
    for (index, chip) in chips.iter().enumerate() {
        if chip.is_none() {
            issues.push(format!("{key}.{index}: unknown widget name"));
        }
    }
    if chips.iter().any(Option::is_none) {
        return Config::default().chips;
    }
    let valid: Vec<Chip> = chips.into_iter().flatten().collect();

    let mut seen = HashSet::new();
    if valid.iter().any(|chip| !seen.insert(chip.name())) {
        issues.push(format!("{key}: must not repeat a widget"));
        return Config::default().chips;
    }
    valid
}

fn parse_mounts(object: &Map<String, Value>, issues: &mut Vec<String>) -> Vec<String> {
    let key = "mounts";
    let Some(value) = object.get(key).filter(|value| !value.is_null()) else {
        return Config::default().mounts;
    };
    let Some(items) = value.as_array() else {
        issues.push(format!("{key}: must be a list of mount points"));
        return Config::default().mounts;
    };
    if items.is_empty() {
        issues.push(format!("{key}: must include at least one mount point"));
        return Config::default().mounts;
    }
    if items.len() > 8 {
        issues.push(format!("{key}: must include at most 8 mount points"));
        return Config::default().mounts;
    }

    let mut mounts = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let Some(text) = item.as_str() else {
            issues.push(format!("{key}.{index}: must be a path"));
            continue;
        };
        let trimmed = text.trim();
        if !trimmed.starts_with('/') {
            issues.push(format!("{key}.{index}: must be an absolute path"));
        } else if trimmed.chars().count() > 255 {
            issues.push(format!(
                "{key}.{index}: must be a path of at most 255 characters"
            ));
        } else {
            mounts.push(trimmed.to_owned());
        }
    }

    {
        let mut seen = HashSet::new();
        if mounts.iter().any(|mount| !seen.insert(mount.clone())) {
            issues.push(format!("{key}: must not repeat a mount point"));
            return Config::default().mounts;
        }
    }
    if mounts.is_empty() || issues.iter().any(|issue| issue.starts_with(key)) {
        Config::default().mounts
    } else {
        mounts
    }
}

fn parse_display_mode(
    object: &Map<String, Value>,
    key: &str,
    issues: &mut Vec<String>,
) -> DisplayMode {
    match object.get(key) {
        None | Some(Value::Null) => DisplayMode::Percent,
        Some(Value::String(text)) => match text.as_str() {
            "percent" => DisplayMode::Percent,
            "absolute" => DisplayMode::Absolute,
            _ => {
                issues.push(format!("{key}: must be \"percent\" or \"absolute\""));
                DisplayMode::Percent
            }
        },
        Some(_) => {
            issues.push(format!("{key}: must be \"percent\" or \"absolute\""));
            DisplayMode::Percent
        }
    }
}

fn parse_temp_unit(object: &Map<String, Value>, issues: &mut Vec<String>) -> TempUnit {
    let key = "temp-unit";
    match object.get(key) {
        None | Some(Value::Null) => TempUnit::Celsius,
        Some(Value::String(text)) => match text.as_str() {
            "celsius" => TempUnit::Celsius,
            "fahrenheit" => TempUnit::Fahrenheit,
            _ => {
                issues.push(format!("{key}: must be \"celsius\" or \"fahrenheit\""));
                TempUnit::Celsius
            }
        },
        Some(_) => {
            issues.push(format!("{key}: must be \"celsius\" or \"fahrenheit\""));
            TempUnit::Celsius
        }
    }
}

fn parse_thresholds(object: &Map<String, Value>, issues: &mut Vec<String>) -> (f32, f32) {
    const LIMITS: std::ops::RangeInclusive<f64> = 20.0..=120.0;
    let mut warn = None;
    let mut error = None;
    for (key, slot) in [("temp-warn", &mut warn), ("temp-error", &mut error)] {
        if let Some(value) = optional_number(object, key, issues) {
            if LIMITS.contains(&value) {
                *slot = Some(value as f32);
            } else {
                issues.push(format!("{key}: must be a temperature limit between 20 and 120"));
            }
        }
    }
    let (warn, error) = (warn.unwrap_or(75.0), error.unwrap_or(85.0));
    if warn >= error {
        issues.push("temp-warn: must be lower than temp-error".to_owned());
        (75.0, 85.0)
    } else {
        (warn, error)
    }
}

/// Reads a finite number from `key`, pushing an issue when it is present with
/// the wrong type. `null` and a missing key both read as optional.
fn optional_number(object: &Map<String, Value>, key: &str, issues: &mut Vec<String>) -> Option<f64> {
    match object.get(key).filter(|value| !value.is_null()) {
        None => None,
        Some(Value::Number(value)) => match value.as_f64() {
            Some(finite) if finite.is_finite() => Some(finite),
            _ => {
                issues.push(format!("{key}: must be a finite number"));
                None
            }
        },
        Some(_) => {
            issues.push(format!("{key}: must be a number"));
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn errors(value: Value) -> Vec<String> {
        parse(&value).expect_err("expected issues")
    }

    #[test]
    fn empty_options_take_defaults() {
        assert_eq!(parse(&json!({})), Ok(Config::default()));
        assert_eq!(parse(&json!(null)), Ok(Config::default()));
    }

    #[test]
    fn non_object_is_rejected() {
        assert_eq!(errors(json!("nope")), ["settings: must be a settings object"]);
        assert_eq!(errors(json!(7)), ["settings: must be a settings object"]);
    }

    #[test]
    fn interval_checks_boundaries() {
        assert_eq!(errors(json!({ "interval": 0 })), ["interval: must be at least 1"]);
        assert_eq!(errors(json!({ "interval": 61 })), ["interval: must be at most 60"]);
        assert_eq!(errors(json!({ "interval": 2.5 })), ["interval: must be an integer"]);
        assert_eq!(errors(json!({ "interval": "fast" })), ["interval: must be a number"]);
        assert_eq!(parse(&json!({ "interval": 10 })).expect("valid").interval_seconds, 10);
    }

    #[test]
    fn chips_validate_names_order_and_uniqueness() {
        let config = parse(&json!({ "widgets": ["temp", "cpu"] })).expect("valid");
        assert_eq!(config.chips, vec![Chip::Temp, Chip::Cpu]);
        let config = parse(&json!({
            "widgets": ["cpu", "ram", "swap", "disk", "temp", "net", "load", "uptime"],
        }))
        .expect("all eight names valid");
        assert_eq!(
            config.chips,
            vec![
                Chip::Cpu,
                Chip::Ram,
                Chip::Swap,
                Chip::Disk,
                Chip::Temp,
                Chip::Net,
                Chip::Load,
                Chip::Uptime,
            ]
        );
        assert!(errors(json!({ "widgets": ["cpu", "frobnicate"] }))
            .contains(&"widgets.1: unknown widget name".to_owned()));
        assert!(errors(json!({ "widgets": ["cpu", "cpu"] }))
            .contains(&"widgets: must not repeat a widget".to_owned()));
        assert!(errors(json!({ "widgets": [] }))
            .contains(&"widgets: must include at least one widget".to_owned()));
        assert!(errors(json!({ "widgets": "cpu" }))
            .contains(&"widgets: must be a list of widget names".to_owned()));
    }

    #[test]
    fn mounts_validate_paths_and_limits() {
        assert_eq!(
            parse(&json!({ "mounts": ["/", " /home "] })).expect("valid").mounts,
            vec!["/", "/home",]
        );
        assert!(errors(json!({ "mounts": ["home"] }))
            .contains(&"mounts.0: must be an absolute path".to_owned()));
        assert!(errors(json!({ "mounts": [format!("/{}", "a".repeat(256))] }))
            .contains(&"mounts.0: must be a path of at most 255 characters".to_owned()));
        assert!(errors(json!({ "mounts": [] }))
            .contains(&"mounts: must include at least one mount point".to_owned()));
        assert!(errors(json!({ "mounts": ["/a", "/a"] }))
            .contains(&"mounts: must not repeat a mount point".to_owned()));
        assert!(errors(json!({
            "mounts": ["/", "/", "/", "/", "/", "/", "/", "/", "/"],
        }))
        .contains(&"mounts: must include at most 8 mount points".to_owned()));
    }

    #[test]
    fn display_modes_accept_both_words_only() {
        let config = parse(&json!({
            "ram-display": "absolute",
            "swap-display": "percent",
            "disk-display": "absolute",
        }))
        .expect("valid");
        assert_eq!(config.ram_display, DisplayMode::Absolute);
        assert_eq!(config.swap_display, DisplayMode::Percent);
        assert_eq!(config.disk_display, DisplayMode::Absolute);
        assert_eq!(errors(json!({ "ram-display": "pie" }))
            .as_slice(), ["ram-display: must be \"percent\" or \"absolute\""]);
        assert_eq!(errors(json!({ "swap-display": "pie" }))
            .as_slice(), ["swap-display: must be \"percent\" or \"absolute\""]);
        assert_eq!(errors(json!({ "disk-display": 5 }))
            .as_slice(), ["disk-display: must be \"percent\" or \"absolute\""]);
    }

    #[test]
    fn temperature_thresholds_have_defaults_and_bounds() {
        let config = parse(&json!({})).expect("valid");
        assert_eq!(config.temp_warn_c, 75.0);
        assert_eq!(config.temp_error_c, 85.0);
        let config = parse(&json!({ "temp-warn": 60, "temp-error": 110 })).expect("valid");
        assert_eq!(config.temp_warn_c, 60.0);
        assert_eq!(config.temp_error_c, 110.0);
        assert!(errors(json!({ "temp-warn": 10 }))
            .contains(&"temp-warn: must be a temperature limit between 20 and 120".to_owned()));
        assert!(errors(json!({ "temp-warn": 90, "temp-error": 90 }))
            .contains(&"temp-warn: must be lower than temp-error".to_owned()));
        assert!(errors(json!({ "temp-warn": "hot" }))
            .contains(&"temp-warn: must be a number".to_owned()));
    }

    #[test]
    fn every_problem_is_reported_at_once() {
        let issues = errors(json!({ "interval": 0, "ram-display": "pie" }));
        assert_eq!(issues.len(), 2);
    }

    #[test]
    fn temp_unit_defaults_to_celsius_and_accepts_both_words() {
        let config = parse(&json!({})).expect("valid");
        assert_eq!(config.temp_unit, TempUnit::Celsius);
        let config = parse(&json!({ "temp-unit": "fahrenheit" })).expect("valid");
        assert_eq!(config.temp_unit, TempUnit::Fahrenheit);
        assert_eq!(
            errors(json!({ "temp-unit": "kelvin" })),
            ["temp-unit: must be \"celsius\" or \"fahrenheit\""]
        );
        assert_eq!(
            errors(json!({ "temp-unit": 12 })),
            ["temp-unit: must be \"celsius\" or \"fahrenheit\""]
        );
        let issues = errors(json!({ "temp-unit": "kelvin", "interval": 0 }));
        assert_eq!(issues.len(), 2);
    }
}
