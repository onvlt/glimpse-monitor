//! Panel config parsing. Options arrive as a JSON object validated here, not
//! trusted; every violation becomes a message the panel shows in a chip tooltip.

use serde_json::{Map, Value};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Chip {
    Cpu,
    Ram,
    Disk,
    Temp,
}

impl Chip {
    fn from_name(value: &str) -> Option<Self> {
        match value {
            "cpu" => Some(Self::Cpu),
            "ram" => Some(Self::Ram),
            "disk" => Some(Self::Disk),
            "temp" => Some(Self::Temp),
            _ => None,
        }
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Ram => "ram",
            Self::Disk => "disk",
            Self::Temp => "temp",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Ram,
    Disk,
}

/// One applet instance's validated settings.
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    /// Seconds between samples.
    pub interval_seconds: u64,
    /// Chips to draw on the panel, in the configured order.
    pub widgets: Vec<Chip>,
    /// Mount points shown in the storage section, in the configured order.
    pub mounts: Vec<String>,
    /// RAM chip and row style.
    pub ram_display: DisplayMode,
    /// Disk chip and row style.
    pub disk_display: DisplayMode,
    /// Fallback warn threshold when a sensor has no critical limit.
    pub temp_warn_c: f32,
    /// Fallback error threshold when a sensor has no critical limit.
    pub temp_error_c: f32,
}

use crate::format::DisplayMode;

impl Default for Config {
    fn default() -> Self {
        Self {
            interval_seconds: 2,
            widgets: vec![Chip::Cpu, Chip::Ram, Chip::Disk, Chip::Temp],
            mounts: vec!["/".to_owned()],
            ram_display: DisplayMode::Percent,
            disk_display: DisplayMode::Percent,
            temp_warn_c: 75.0,
            temp_error_c: 85.0,
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
    let widgets = parse_widgets(object, &mut issues);
    let mounts = parse_mounts(object, &mut issues);
    let ram_display = parse_display_mode(object, Side::Ram, &mut issues);
    let disk_display = parse_display_mode(object, Side::Disk, &mut issues);
    let (temp_warn_c, temp_error_c) = parse_thresholds(object, &mut issues);

    if issues.is_empty() {
        Ok(Config {
            interval_seconds,
            widgets,
            mounts,
            ram_display,
            disk_display,
            temp_warn_c,
            temp_error_c,
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

fn parse_widgets(object: &Map<String, Value>, issues: &mut Vec<String>) -> Vec<Chip> {
    let key = "widgets";
    let Some(value) = object.get(key).filter(|value| !value.is_null()) else {
        return Config::default().widgets;
    };
    let Some(items) = value.as_array() else {
        issues.push(format!("{key}: must be a list of widget names"));
        return Config::default().widgets;
    };
    if items.is_empty() {
        issues.push(format!("{key}: must include at least one widget"));
        return Config::default().widgets;
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
        return Config::default().widgets;
    }
    let valid: Vec<Chip> = chips.into_iter().flatten().collect();

    let mut seen = HashSet::new();
    if valid.iter().any(|chip| !seen.insert(chip.name())) {
        issues.push(format!("{key}: must not repeat a widget"));
        return Config::default().widgets;
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

fn parse_display_mode(object: &Map<String, Value>, side: Side, issues: &mut Vec<String>) -> DisplayMode {
    let key = match side {
        Side::Ram => "ram-display",
        Side::Disk => "disk-display",
    };
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

/// Reads a finite number from `key`, pushing an issue when it is absent-but-
/// present-with-the-wrong-type. `null` and a missing key both read as optional.
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
    fn widgets_validate_names_order_and_uniqueness() {
        assert_eq!(
            parse(&json!({ "widgets": ["temp", "cpu"] })).expect("valid").widgets,
            vec![Chip::Temp, Chip::Cpu]
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
        assert!(errors(json!({ "mounts": ["/", "/", "/", "/", "/", "/", "/", "/", "/"] }))
            .contains(&"mounts: must include at most 8 mount points".to_owned()));
    }

    #[test]
    fn display_modes_accept_both_words_only() {
        let config = parse(&json!({ "ram-display": "absolute", "disk-display": "absolute" }))
            .expect("valid");
        assert_eq!(config.ram_display, DisplayMode::Absolute);
        assert_eq!(config.disk_display, DisplayMode::Absolute);
        assert_eq!(errors(json!({ "ram-display": "pie" }))
            .as_slice(), ["ram-display: must be \"percent\" or \"absolute\""]);
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
}
