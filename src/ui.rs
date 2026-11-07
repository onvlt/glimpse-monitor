//! Widget tree assembly: the panel chips and the popover sections.

use crate::config::{Chip, Config};
use crate::format::{self, DisplayMode, SizeUnit, TempUnit};
use crate::snapshot::{
    Shared, Snapshot, TempReading, Tone, USAGE_ERROR_PCT, USAGE_WARN_PCT, temp_tone, usage_tone,
};
use glimpse_sdk::*;

const CHIP_ICONS: [(Chip, &str); 4] = [
    (Chip::Cpu, "glimpse-monitor-cpu-symbolic"),
    (Chip::Ram, "glimpse-monitor-memory-symbolic"),
    (Chip::Disk, "glimpse-monitor-storage-symbolic"),
    (Chip::Temp, "glimpse-monitor-temperature-symbolic"),
];
const POPOVER_ICON: &str = "glimpse-monitor-system-symbolic";

/// The whole tree: the error chip, the data chips and the popover.
pub fn tree(
    config: &Result<Config, Vec<String>>,
    shared: Option<&Shared>,
) -> Vec<Widget<crate::app::Msg>> {
    let config = match config {
        Err(issues) => return vec![error_chip(issues)],
        Ok(config) => config,
    };
    let empty = Snapshot::default();
    let snap = shared.map(Shared::snapshot).unwrap_or(empty);
    let unit = format::resolve_temp_unit(format::locale_from_env().as_deref());

    let mut widgets: Vec<Widget<crate::app::Msg>> = config
        .widgets
        .iter()
        .map(|chip| {
            indicator()
                .key(chip.name())
                .icon(chip_icon(*chip))
                .severity(chip_severity(*chip, &snap, config, unit))
                .tooltip(chip_tooltip(*chip, &snap, config, unit))
                .text(chip_text(*chip, &snap, config, unit))
                .into()
        })
        .collect();
    widgets.push(popover(snap, config, unit));
    widgets
}

fn error_chip(issues: &[String]) -> Widget<crate::app::Msg> {
    indicator()
        .icon(POPOVER_ICON)
        .severity(Severity::Error)
        .tooltip(issues.join("\n"))
        .text("invalid settings")
        .into()
}

fn chip_icon(chip: Chip) -> &'static str {
    CHIP_ICONS
        .iter()
        .find_map(|(known, icon)| (*known == chip).then_some(*icon))
        .unwrap_or(POPOVER_ICON)
}

fn popover(snap: Snapshot, config: &Config, unit: TempUnit) -> Widget<crate::app::Msg> {
    glimpse_sdk::popover()
        .title("System monitor")
        .icon(POPOVER_ICON)
        .child(processor_section(&snap))
        .child(memory_section(&snap, config))
        .child(storage_section(&snap, config))
        .child(temperature_section(&snap, config, unit))
        .into()
}

fn processor_section(snap: &Snapshot) -> Widget<crate::app::Msg> {
    let pct = snap.cpu_pct;
    section()
        .title("Processor")
        .child(
            meter_row()
                .icon(chip_icon(Chip::Cpu))
                .title("CPU")
                .value(pct.map_or_else(|| "…".to_owned(), format::pct_text))
                .level(Some(pct.map_or(0.0, |pct| pct / 100.0)))
                .typography(RowTypography::Numeric),
        )
        .child(
            sparkline()
                .values(snap.history.iter().copied())
                .fill(true)
                .show_last(true)
                .label("CPU history"),
        )
        .into()
}

fn memory_section(snap: &Snapshot, config: &Config) -> Widget<crate::app::Msg> {
    let Some(ram) = snap.ram.as_ref() else {
        return section()
            .title("Memory")
            .child(
                meter_row()
                    .icon(chip_icon(Chip::Ram))
                    .title("RAM")
                    .value("…")
                    .typography(RowTypography::Numeric),
            )
            .into();
    };

    let tone = usage_tone(ram.pct);
    let mut row = meter_row()
        .icon(chip_icon(Chip::Ram))
        .title("RAM")
        .value(format::row_value(ram.used, ram.total, config.ram_display, SizeUnit::Binary))
        .subtitle(format::row_subtitle(
            ram.used,
            ram.total,
            config.ram_display,
            SizeUnit::Binary,
        ))
        .level(Some(ram.pct / 100.0))
        .typography(RowTypography::Numeric);
    row = tone_row(row, tone);
    section().title("Memory").child(row).into()
}

fn storage_section(snap: &Snapshot, config: &Config) -> Widget<crate::app::Msg> {
    let mut section = section().title("Disks");
    if snap.disks.is_empty() {
        section = section.child(
            placeholder()
                .icon(chip_icon(Chip::Disk))
                .title("No mounts found")
                .description("Monitor extra disks with the mounts option"),
        );
    } else {
        for (index, disk) in snap.disks.iter().enumerate() {
            let pct = format::usage_pct(disk.used, disk.total);
            let tone = usage_tone(pct);
            let mut row = meter_row()
                .key(disk.mount.clone())
                .icon(chip_icon(Chip::Disk))
                .title(if index == 0 {
                    "Disk".to_owned()
                } else {
                    disk.mount.clone()
                })
                .value(format::row_value(
                    disk.used,
                    disk.total,
                    config.disk_display,
                    SizeUnit::Decimal,
                ))
                .subtitle(format::row_subtitle(
                    disk.used,
                    disk.total,
                    config.disk_display,
                    SizeUnit::Decimal,
                ))
                .level(Some(pct / 100.0))
                .typography(RowTypography::Numeric);
            row = tone_row(row, tone);
            section = section.child(row);
        }
    }
    section.into()
}

fn temperature_section(snap: &Snapshot, config: &Config, unit: TempUnit) -> Widget<crate::app::Msg> {
    let mut section = section().title("Temperature");
    if snap.temps.is_empty() {
        section = section.child(
            row()
                .icon(chip_icon(Chip::Temp))
                .title("Temperature")
                .value("—")
                .subtitle("no sensor found")
                .inactive(true),
        );
    } else {
        for reading in snap.temps.iter() {
            let tone = temp_tone(reading, config.temp_warn_c, config.temp_error_c);
            let mut element = row()
                .key(reading.label.clone())
                .icon(chip_icon(Chip::Temp))
                .title(reading.label.clone())
                .value(format::format_temp(reading.celsius, unit))
                .typography(RowTypography::Numeric);
            if let Some(subtitle) = temp_subtitle(reading, unit) {
                element = element.subtitle(subtitle);
            }
            element = tone_row(element, tone);
            section = section.child(element);
        }
    }
    section.into()
}

fn temp_subtitle(reading: &TempReading, unit: TempUnit) -> Option<String> {
    match (reading.critical_c, reading.peak_c) {
        (Some(critical), _) => Some(format!("critical {}", format::format_temp(critical, unit))),
        (None, Some(peak)) => Some(format!("peak {}", format::format_temp(peak, unit))),
        (None, None) => None,
    }
}

fn tone_row<M, T>(row: T, tone: Tone) -> T
where
    T: ToneRow<M>,
{
    match tone {
        Tone::None => row,
        Tone::Warning => row.warning(),
        Tone::Error => row.error(),
    }
}

/// Rows that accept a tone.
trait ToneRow<M> {
    fn warning(self) -> Self;
    fn error(self) -> Self;
}

macro_rules! impl_tone_row {
    ($row:ident) => {
        impl<M> ToneRow<M> for $row<M> {
            fn warning(self) -> Self {
                self.tone(RowTone::Warning)
            }
            fn error(self) -> Self {
                self.tone(RowTone::Error)
            }
        }
    };
}

impl_tone_row!(Row);
impl_tone_row!(MeterRow);

/// Chip text: `12%`, `4.2/16G`, `43°`.
#[must_use]
pub fn chip_text(chip: Chip, snap: &Snapshot, config: &Config, unit: TempUnit) -> String {
    match chip {
        Chip::Cpu => snap.cpu_pct.map_or_else(|| "…".to_owned(), format::pct_text),
        Chip::Ram => snap.ram.as_ref().map_or_else(
            || "…".to_owned(),
            |ram| match config.ram_display {
                DisplayMode::Percent => format::pct_text(ram.pct),
                DisplayMode::Absolute => format!(
                    "{}/{}",
                    format::format_compact(ram.used, SizeUnit::Binary),
                    format::format_compact(ram.total, SizeUnit::Binary)
                ),
            },
        ),
        Chip::Disk => snap.disks.first().map_or_else(
            || "…".to_owned(),
            |disk| match config.disk_display {
                DisplayMode::Percent => format::pct_text(format::usage_pct(disk.used, disk.total)),
                DisplayMode::Absolute => format!(
                    "{}/{}",
                    format::format_compact(disk.used, SizeUnit::Decimal),
                    format::format_compact(disk.total, SizeUnit::Decimal)
                ),
            },
        ),
        Chip::Temp => snap.temps.first().map_or_else(
            || "…".to_owned(),
            |reading| format!("{}°", format::display_temp(reading.celsius, unit)),
        ),
    }
}

/// Fuller chip text for the tooltip: `RAM 4.2 GiB of 16 GiB`.
#[must_use]
pub fn chip_tooltip(chip: Chip, snap: &Snapshot, config: &Config, unit: TempUnit) -> String {
    match chip {
        Chip::Cpu => snap.cpu_pct.map_or_else(
            || "CPU …".to_owned(),
            |pct| format!("CPU {}", format::pct_text(pct)),
        ),
        Chip::Ram => snap.ram.as_ref().map_or_else(
            || "RAM …".to_owned(),
            |ram| match config.ram_display {
                DisplayMode::Absolute => format!(
                    "RAM {} of {}",
                    format::format_full(ram.used, SizeUnit::Binary),
                    format::format_full(ram.total, SizeUnit::Binary)
                ),
                DisplayMode::Percent => format!("RAM {}", format::pct_text(ram.pct)),
            },
        ),
        Chip::Disk => snap.disks.first().map_or_else(
            || "Disk …".to_owned(),
            |disk| match config.disk_display {
                DisplayMode::Absolute => format!(
                    "Disk {} of {}",
                    format::format_full(disk.used, SizeUnit::Decimal),
                    format::format_full(disk.total, SizeUnit::Decimal)
                ),
                DisplayMode::Percent => format!(
                    "Disk {}",
                    format::pct_text(format::usage_pct(disk.used, disk.total))
                ),
            },
        ),
        Chip::Temp => snap.temps.first().map_or_else(
            || "Temp …".to_owned(),
            |reading| {
                format!(
                    "Temp {} — {}",
                    format::format_temp_full(reading.celsius, unit),
                    reading.label
                )
            },
        ),
    }
}

/// Warning/error coloring for one chip: readings that run high.
#[must_use]
pub fn chip_severity(chip: Chip, snap: &Snapshot, config: &Config, unit: TempUnit) -> Severity {
    unchecked_by_unit(chip, snap, config, unit)
}

fn unchecked_by_unit(
    chip: Chip,
    snap: &Snapshot,
    config: &Config,
    _unit: TempUnit,
) -> Severity {
    let level = |pct: Option<f64>| match pct {
        Some(pct) if pct >= USAGE_ERROR_PCT => Severity::Error,
        Some(pct) if pct >= USAGE_WARN_PCT => Severity::Warning,
        _ => Severity::Info,
    };
    match chip {
        Chip::Cpu => level(snap.cpu_pct),
        Chip::Ram => level(snap.ram.as_ref().map(|ram| ram.pct)),
        Chip::Disk => level(
            snap.disks
                .first()
                .map(|disk| format::usage_pct(disk.used, disk.total)),
        ),
        Chip::Temp => match snap.temps.first().map(|reading| {
            temp_tone(reading, config.temp_warn_c, config.temp_error_c)
        }) {
            None | Some(Tone::None) => Severity::Info,
            Some(Tone::Warning) => Severity::Warning,
            Some(Tone::Error) => Severity::Error,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Snapshot {
        Snapshot {
            cpu_pct: Some(12.0),
            ram: crate::snapshot::ram_info(17_179_869_184, 12_344_554_992),
            disks: vec![crate::snapshot::DiskUsage {
                mount: "/".to_owned(),
                used: 4_512_515_072,
                total: 17_179_869_184,
            }],
            temps: vec![TempReading {
                label: "Package id 0".to_owned(),
                celsius: 43.0,
                critical_c: None,
                peak_c: None,
            }],
            history: vec![Some(12.0)],
        }
    }

    #[test]
    fn chips_render_short_values() {
        let snap = fixture();
        let config = Config::default();
        let unit = TempUnit::Celsius;
        assert_eq!(chip_text(Chip::Cpu, &snap, &config, unit), "12%");
        assert_eq!(chip_text(Chip::Ram, &snap, &config, unit), "28%");
        assert_eq!(chip_text(Chip::Disk, &snap, &config, unit), "26%");
        assert_eq!(chip_text(Chip::Temp, &snap, &config, unit), "43°");
        for chip in [Chip::Cpu, Chip::Ram, Chip::Disk, Chip::Temp] {
            assert_eq!(chip_text(chip, &Snapshot::default(), &config, unit), "…");
        }
    }

    #[test]
    fn absolute_mode_swaps_chip_sizes() {
        let snap = fixture();
        let config = Config {
            ram_display: DisplayMode::Absolute,
            disk_display: DisplayMode::Absolute,
            ..Config::default()
        };
        let unit = TempUnit::Celsius;
        assert_eq!(chip_text(Chip::Ram, &snap, &config, unit), "4.5G/16G");
        assert_eq!(chip_text(Chip::Disk, &snap, &config, unit), "4.5G/17.2G");
        assert_eq!(
            chip_tooltip(Chip::Disk, &snap, &config, unit),
            "Disk 4.51 GB of 17.18 GB"
        );
    }

    #[test]
    fn tooltips_fully_describe_the_reading() {
        let snap = fixture();
        let config = Config::default();
        let unit = TempUnit::Celsius;
        assert_eq!(chip_tooltip(Chip::Cpu, &snap, &config, unit), "CPU 12%");
        assert_eq!(
            chip_tooltip(Chip::Ram, &snap, &config, unit),
            "RAM 28%"
        );
        assert_eq!(chip_tooltip(Chip::Disk, &snap, &config, unit), "Disk 26%");
        assert_eq!(
            chip_tooltip(Chip::Temp, &snap, &config, unit),
            "Temp 43 °C — Package id 0"
        );
    }

    #[test]
    fn severities_follow_the_thresholds() {
        let config = Config::default();
        let unit = TempUnit::Celsius;
        for chip in [Chip::Cpu, Chip::Ram, Chip::Disk, Chip::Temp] {
            assert_eq!(
                chip_severity(chip, &Snapshot::default(), &config, unit),
                Severity::Info
            );
        }

        let mut snap = fixture();
        assert_eq!(chip_severity(Chip::Disk, &snap, &config, unit), Severity::Info);
        snap.cpu_pct = Some(96.0);
        assert_eq!(chip_severity(Chip::Cpu, &snap, &config, unit), Severity::Error);
        snap.cpu_pct = Some(90.0);
        assert_eq!(chip_severity(Chip::Cpu, &snap, &config, unit), Severity::Warning);
        snap.temps[0].celsius = 76.0;
        assert_eq!(chip_severity(Chip::Temp, &snap, &config, unit), Severity::Warning);
        snap.temps[0].celsius = 86.0;
        assert_eq!(chip_severity(Chip::Temp, &snap, &config, unit), Severity::Error);
    }

    #[test]
    fn tree_holds_a_chip_and_the_popover() {
        let config = Config::default();
        let shared = Shared::default();
        let widgets = tree(&Ok(config), Some(&shared));
        assert_eq!(widgets.len(), 5); // one chip per default widget + popover

        let issues = crate::config::parse(&serde_json::json!({ "interval": 0 }))
            .expect_err("invalid");
        let widgets = tree(&Err(issues), None);
        assert_eq!(widgets.len(), 1); // the error chip alone
    }
}
