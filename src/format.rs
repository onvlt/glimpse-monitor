//! Text and unit formatting helpers. Everything here is pure and easy to test.

const MIB: f64 = 1024.0 * 1024.0;
const GIB: f64 = 1024.0 * MIB;
const GB: f64 = 1000.0 * 1000.0 * 1000.0;
const MB: f64 = 1000.0 * 1000.0;

/// How sizes are scaled: `1024`-based ("GiB") or `1000`-based ("GB").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizeUnit {
    Binary,
    Decimal,
}

/// Whether percentages or absolute sizes are shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayMode {
    Percent,
    Absolute,
}

/// Temperature scale used by display text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TempUnit {
    Celsius,
    Fahrenheit,
}

impl TempUnit {
    /// `°C` / `°F` suffix used by row and chip text.
    #[must_use]
    pub fn suffix(self) -> &'static str {
        match self {
            Self::Celsius => "C",
            Self::Fahrenheit => "F",
        }
    }
}

/// Formats a number with fixed decimals and drops trailing zeros,
/// so `4.20` becomes `4.2` and `16.00` becomes `16`.
#[must_use]
pub fn fixed_trimmed(value: f64, digits: usize) -> String {
    let text = format!("{value:.digits$}");
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text
    }
}

/// `4.2 GiB`, `512 MiB`, `9.5 GB`, `402 MB`.
#[must_use]
pub fn format_full(bytes: u64, style: SizeUnit) -> String {
    let value = bytes as f64;
    match style {
        SizeUnit::Decimal => {
            if value >= GB {
                format!("{} GB", fixed_trimmed(value / GB, 2))
            } else {
                format!("{} MB", fixed_trimmed(value / MB, 2))
            }
        }
        SizeUnit::Binary => {
            if value >= GIB {
                format!("{} GiB", fixed_trimmed(value / GIB, 2))
            } else {
                format!("{} MiB", fixed_trimmed(value / MIB, 2))
            }
        }
    }
}

/// `4.2G`, `16G`, `402M` for the chip.
#[must_use]
pub fn format_compact(bytes: u64, style: SizeUnit) -> String {
    let value = bytes as f64;
    match style {
        SizeUnit::Decimal => {
            if value >= GB {
                format!("{}G", fixed_trimmed(value / GB, 1))
            } else {
                format!("{}M", fixed_trimmed(value / MB, 1))
            }
        }
        SizeUnit::Binary => {
            if value >= GIB {
                format!("{}G", fixed_trimmed(value / GIB, 1))
            } else {
                format!("{}M", fixed_trimmed(value / MIB, 1))
            }
        }
    }
}

/// A share of `0.0..=1.0` clamped out of a used/total pair.
#[must_use]
pub fn usage_level(used: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    (used as f64 / total as f64).clamp(0.0, 1.0)
}

/// A percentage of `0.0..=100.0` clamped out of a used/total pair.
#[must_use]
pub fn usage_pct(used: u64, total: u64) -> f64 {
    usage_level(used, total).clamp(0.0, 100.0) * 100.0
}

/// `12%`.
#[must_use]
pub fn pct_text(pct: f64) -> String {
    format!("{}%", pct.round() as i64)
}

/// A whole temperature value: `43` (Celsius) or `110` (Fahrenheit).
#[must_use]
pub fn display_temp(celsius: f32, unit: TempUnit) -> i64 {
    let value = match unit {
        TempUnit::Celsius => celsius,
        TempUnit::Fahrenheit => celsius * 9.0 / 5.0 + 32.0,
    };
    value.round() as i64
}

/// `1.2M`, `430k` — bytes per second without the unit, decimal sizes.
#[must_use]
pub fn speed_compact(bytes_per_s: f64) -> String {
    let value = bytes_per_s.max(0.0);
    if value >= 1_000_000.0 {
        format!("{}M", fixed_trimmed(value / 1_000_000.0, 1))
    } else if value >= 1_000.0 {
        format!("{}k", fixed_trimmed(value / 1_000.0, 1))
    } else {
        format!("{}", value.round() as i64)
    }
}

/// `1.2M/s`, `430k/s` — a byte rate in decimal sizes.
#[must_use]
pub fn speed_text(bytes_per_s: f64) -> String {
    format!("{}/s", speed_compact(bytes_per_s))
}

/// `16d 3h`, `5h 12m`, `40m` — a duration split into its two coarsest units.
#[must_use]
pub fn uptime_text(seconds: u64) -> String {
    let minutes = seconds / 60;
    if minutes < 60 {
        format!("{minutes}m")
    } else if minutes < 24 * 60 {
        let hours = minutes / 60;
        format!("{}h {}m", hours, minutes % 60)
    } else {
        let days = minutes / (24 * 60);
        format!("{}d {}h", days, (minutes % (24 * 60)) / 60)
    }
}

/// `0.58`, `1` — a load average value trimmed to two decimals.
#[must_use]
pub fn load_text(value: f64) -> String {
    fixed_trimmed(value, 2)
}

/// `43°C`.
#[must_use]
pub fn format_temp(celsius: f32, unit: TempUnit) -> String {
    format!("{}°{}", display_temp(celsius, unit), unit.suffix())
}

/// A temperature with one decimal in the reading's own unit: `43.4 °C`.
#[must_use]
pub fn format_temp_full(celsius: f32, unit: TempUnit) -> String {
    match unit {
        TempUnit::Celsius => format!("{} °C", fixed_trimmed(f64::from(celsius), 1)),
        TempUnit::Fahrenheit => format!("{} °F", display_temp(celsius, unit)),
    }
}

/// Row end text: `42%` or `4.2 GiB / 16 GiB` depending on the mode.
#[must_use]
pub fn row_value(used: u64, total: u64, mode: DisplayMode, style: SizeUnit) -> String {
    match mode {
        DisplayMode::Absolute => {
            format!("{0} / {1}", format_full(used, style), format_full(total, style))
        }
        DisplayMode::Percent => pct_text(usage_pct(used, total)),
    }
}

/// Row subtitle: the complementary view of [`row_value`].
#[must_use]
pub fn row_subtitle(used: u64, total: u64, mode: DisplayMode, style: SizeUnit) -> String {
    let free = total.saturating_sub(used);
    match mode {
        DisplayMode::Absolute => format!(
            "{} used, {} free",
            pct_text(usage_pct(used, total)),
            format_full(free, style)
        ),
        DisplayMode::Percent => format!(
            "{} of {} used",
            format_full(used, style),
            format_full(total, style)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_trimmed_drops_useless_zeros() {
        assert_eq!(fixed_trimmed(4.2, 2), "4.2");
        assert_eq!(fixed_trimmed(16.0, 2), "16");
        assert_eq!(fixed_trimmed(0.0, 1), "0");
        assert_eq!(fixed_trimmed(402.25, 2), "402.25");
    }

    #[test]
    fn full_sizes_trim_trailing_zeros() {
        assert_eq!(format_full(4_512_515_072, SizeUnit::Binary), "4.2 GiB");
        assert_eq!(format_full(17_179_869_184, SizeUnit::Binary), "16 GiB");
        assert_eq!(format_full(536_870_912, SizeUnit::Binary), "512 MiB");
        assert_eq!(format_full(9_500_000_000, SizeUnit::Decimal), "9.5 GB");
        assert_eq!(format_full(402_000_000, SizeUnit::Decimal), "402 MB");
    }

    #[test]
    fn compact_sizes() {
        assert_eq!(format_compact(4_512_515_072, SizeUnit::Binary), "4.2G");
        assert_eq!(format_compact(17_179_869_184, SizeUnit::Binary), "16G");
        assert_eq!(format_compact(402_000_000, SizeUnit::Decimal), "402M");
        assert_eq!(format_compact(9_500_000_000, SizeUnit::Decimal), "9.5G");
    }

    #[test]
    fn percentages_and_levels() {
        assert_eq!(usage_pct(25, 100), 25.0);
        assert_eq!(usage_pct(0, 0), 0.0);
        assert_eq!(usage_pct(150, 100), 100.0);
        assert_eq!(usage_pct(25, 0), 0.0);
        assert!((usage_level(1, 3) - 0.333).abs() < 0.001);
        assert_eq!(pct_text(66.6), "67%");
        assert_eq!(pct_text(12.0), "12%");
    }

    #[test]
    fn temperatures_display_and_convert() {
        assert_eq!(display_temp(43.2, TempUnit::Celsius), 43);
        assert_eq!(display_temp(43.2, TempUnit::Fahrenheit), 110);
        assert_eq!(format_temp(43.2, TempUnit::Celsius), "43°C");
        assert_eq!(format_temp(43.2, TempUnit::Fahrenheit), "110°F");
        assert_eq!(format_temp_full(43.44, TempUnit::Celsius), "43.4 °C");
    }

    #[test]
    fn speeds_format_decimal() {
        assert_eq!(speed_compact(0.0), "0");
        assert_eq!(speed_compact(430.0), "430");
        assert_eq!(speed_compact(1_230.0), "1.2k");
        assert_eq!(speed_compact(430_000.0), "430k");
        assert_eq!(speed_compact(1_230_000.0), "1.2M");
        assert_eq!(speed_text(430_000.0), "430k/s");
        assert_eq!(speed_text(-5.0), "0/s");
    }

    #[test]
    fn uptime_splits_into_two_coarsest_units() {
        assert_eq!(uptime_text(40 * 60), "40m");
        assert_eq!(uptime_text(5 * 3600 + 12 * 60), "5h 12m");
        assert_eq!(uptime_text(16 * 86400 + 3 * 3600 + 7 * 60), "16d 3h");
        assert_eq!(uptime_text(0), "0m");
    }

    #[test]
    fn load_values_trim_to_two_decimals() {
        assert_eq!(load_text(0.5812), "0.58");
        assert_eq!(load_text(1.0), "1");
        assert_eq!(load_text(0.0), "0");
    }

    #[test]
    fn row_text_pairs_value_with_subtitle() {
        let used = 4_512_515_072_u64;
        let total = 17_179_869_184_u64;
        assert_eq!(
            row_value(used, total, DisplayMode::Percent, SizeUnit::Binary),
            "26%"
        );
        assert_eq!(
            row_value(used, total, DisplayMode::Absolute, SizeUnit::Binary),
            "4.2 GiB / 16 GiB"
        );
        assert_eq!(
            row_subtitle(used, total, DisplayMode::Percent, SizeUnit::Binary),
            "4.2 GiB of 16 GiB used"
        );
        assert_eq!(
            row_value(used, total, DisplayMode::Percent, SizeUnit::Decimal),
            "26%"
        );
        assert_eq!(
            row_value(used, total, DisplayMode::Absolute, SizeUnit::Decimal),
            "4.51 GB / 17.18 GB"
        );
        assert_eq!(
            row_subtitle(used, total, DisplayMode::Percent, SizeUnit::Decimal),
            "4.51 GB of 17.18 GB used"
        );
        assert_eq!(
            row_subtitle(used, total, DisplayMode::Absolute, SizeUnit::Decimal),
            "26% used, 12.67 GB free"
        );
    }
}
