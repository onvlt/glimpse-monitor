//! Widget tree assembly: the panel chips and the popover sections.

use crate::config::{Chip, Config};
use crate::format::{self, DisplayMode, SizeUnit, TempUnit};
use crate::snapshot::{
    GpuReading, RamInfo, Shared, Snapshot, TempReading, Tone, USAGE_ERROR_PCT, USAGE_WARN_PCT,
    temp_tone, usage_tone,
};
use glimpse_sdk::*;

const CHIP_ICONS: [(Chip, &str); 8] = [
    (Chip::Cpu, "glimpse-monitor-cpu-symbolic"),
    (Chip::Ram, "glimpse-monitor-memory-symbolic"),
    (Chip::Swap, "glimpse-monitor-swap-symbolic"),
    (Chip::Disk, "glimpse-monitor-storage-symbolic"),
    (Chip::Temp, "glimpse-monitor-temperature-symbolic"),
    (Chip::Net, "glimpse-monitor-network-symbolic"),
    (Chip::Load, "glimpse-monitor-system-symbolic"),
    (Chip::Uptime, "glimpse-monitor-system-symbolic"),
];
const POPOVER_ICON: &str = "glimpse-monitor-system-symbolic";
/// Row icon for the Graphics section (never a chip).
const GPU_ROW_ICON: &str = "glimpse-monitor-gpu-symbolic";

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

    let mut nodes: Vec<Widget<crate::app::Msg>> = config
        .chips
        .iter()
        .map(|chip| {
            indicator()
                .key(chip.name())
                .icon(chip_icon(*chip))
                .severity(chip_severity(*chip, &snap, config))
                .tooltip(chip_tooltip(*chip, &snap, config, unit))
                .text(chip_text(*chip, &snap, config, unit))
                .into()
        })
        .collect();
    nodes.push(popover(snap, config, unit));
    nodes
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
    let mut card = glimpse_sdk::popover()
        .title("System monitor")
        .icon(POPOVER_ICON)
        .child(processor_section(&snap))
        .child(memory_section(&snap, config))
        .child(storage_section(&snap, config))
        .child(temperature_section(&snap, config, unit))
        .child(network_section(&snap));
    if !snap.gpus.is_empty() {
        card = card.child(gpu_section(&snap));
    }
    card = card.child(system_section(&snap));
    card.into()
}

fn processor_section(snap: &Snapshot) -> Widget<crate::app::Msg> {
    let pct = snap.cpu_pct;
    let mut section = section()
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
        );
    if !snap.cores.is_empty() {
        section = section.child(
            bars()
                .values(snap.cores.iter().copied().map(Some))
                .maximum(Some(100.0))
                .label("CPU load per core"),
        );
    }
    section.into()
}

fn memory_section(snap: &Snapshot, config: &Config) -> Widget<crate::app::Msg> {
    let mut section = section().title("Memory");
    match snap.ram.clone() {
        None => {
            section = section.child(
                meter_row()
                    .icon(chip_icon(Chip::Ram))
                    .title("RAM")
                    .value("…")
                    .typography(RowTypography::Numeric),
            );
        }
        Some(ram) => {
            section = section.child(capacity_row("RAM", &ram, None, config.ram_display));
        }
    }
    if let Some(swap) = snap.swap.clone() {
        section = section.child(capacity_row("Swap", &swap, None, config.swap_display));
    }
    section.into()
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
            let title = if index == 0 {
                "Disk".to_owned()
            } else {
                disk.mount.clone()
            };
            section = section.child(capacity_row(
                title,
                &crate::snapshot::RamInfo {
                    pct: format::usage_pct(disk.used, disk.total),
                    used: disk.used,
                    total: disk.total,
                },
                Some(disk.mount.clone()),
                config.disk_display,
            ));        }
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
        for reading in &snap.temps {
            let tone = temp_tone(reading, config.temp_warn_c, config.temp_error_c);
            let mut element: Row<crate::app::Msg> = row()
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

fn network_section(snap: &Snapshot) -> Widget<crate::app::Msg> {
    let mut section = section().title("Network");
    if snap.net.is_empty() {
        section = section.child(
            placeholder()
                .icon(chip_icon(Chip::Net))
                .title("No network activity")
                .description("Visible interfaces report here"),
        );
    } else {
        for sample in &snap.net {
            section = section.child(
                row()
                    .key(sample.name.clone())
                    .icon(chip_icon(Chip::Net))
                    .title(sample.name.clone())
                    .value(format::speed_text(sample.down_speed))
                    .subtitle(format::speed_text(sample.up_speed))
                    .tooltip(format!(
                        "Down {} total · Up {} total",
                        format::format_full(sample.down_total, SizeUnit::Decimal),
                        format::format_full(sample.up_total, SizeUnit::Decimal)
                    ))
                    .typography(RowTypography::Numeric),
            );
        }
    }
    section.into()
}

fn gpu_section(snap: &Snapshot) -> Widget<crate::app::Msg> {
    let mut section = section().title("Graphics");
    for gpu in &snap.gpus {
        let mut element = meter_row()
            .key(gpu.model.clone())
            .icon(GPU_ROW_ICON)
            .title(gpu_title(gpu))
            .value(gpu.usage.map_or_else(|| "…".to_owned(), format::pct_text))
            .level(Some(gpu.usage.map_or(0.0, |usage| usage / 100.0)))
            .typography(RowTypography::Numeric);
        if let Some(subtitle) = gpu_memory_subtitle(gpu) {
            element = element.subtitle(subtitle);
        }
        if let Some(usage) = gpu.usage {
            element = tone_row(element, usage_tone(usage));
        }
        section = section.child(element);
    }
    section.into()
}

fn system_section(snap: &Snapshot) -> Widget<crate::app::Msg> {
    let mut facts = Vec::new();
    if let Some(seconds) = snap.uptime_seconds {
        facts.push(Fact::new("Uptime", format::uptime_text(seconds)));
    }
    if let Some(load) = snap.load {
        facts.push(Fact::new(
            "Load",
            format!(
                "{} / {} / {}",
                format::load_text(load.one),
                format::load_text(load.five),
                format::load_text(load.fifteen)
            ),
        ));
    }
    if let Some(info) = snap.system_info.as_ref() {
        for fact in [
            ("OS", info.os.as_deref()),
            ("Kernel", info.kernel.as_deref()),
            ("Host", info.hostname.as_deref()),
        ] {
            if let Some(value) = fact.1.filter(|value| !value.is_empty()) {
                facts.push(Fact::new(fact.0, value));
            }
        }
    }
    let mut section = section().title("System");
    if facts.is_empty() {
        section = section.child(
            row()
                .icon(POPOVER_ICON)
                .title("System")
                .value("—")
                .subtitle("no information")
                .inactive(true),
        );
    } else {
        section = section.child(fact_list().facts(facts));
    }
    section.into()
}

#[allow(clippy::needless_pass_by_value)]
fn capacity_row(
    title: impl Into<String>,
    info: &RamInfo,
    key: Option<String>,
    display: DisplayMode,
) -> Widget<crate::app::Msg> {
    let tone = usage_tone(info.pct);
    let mut element = meter_row()
        .title(title)
        .value(format::row_value(info.used, info.total, display, SizeUnit::Binary))
        .subtitle(format::row_subtitle(
            info.used,
            info.total,
            display,
            SizeUnit::Binary,
        ))
        .level(Some(info.pct / 100.0))
        .typography(RowTypography::Numeric);
    if let Some(key) = key {
        element = element.key(key);
    }
    tone_row(element, tone).into()
}

fn temp_subtitle(reading: &TempReading, unit: TempUnit) -> Option<String> {
    match (reading.critical_c, reading.peak_c) {
        (Some(critical), _) => Some(format!("critical {}", format::format_temp(critical, unit))),
        (None, Some(peak)) => Some(format!("peak {}", format::format_temp(peak, unit))),
        (None, None) => None,
    }
}

fn gpu_title(gpu: &GpuReading) -> String {
    if gpu.model.is_empty() {
        "GPU".to_owned()
    } else {
        gpu.model.clone()
    }
}

/// `8.1 GiB of 15.5 GiB used`, only when both ends of the pair exist.
fn gpu_memory_subtitle(gpu: &GpuReading) -> Option<String> {
    let (used, total) = (gpu.memory_used?, gpu.memory_total?);
    if total == 0 {
        return None;
    }
    Some(format!(
        "{} of {} used",
        format::format_full(used, SizeUnit::Binary),
        format::format_full(total, SizeUnit::Binary)
    ))
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

/// Elements that accept a row tone.
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

/// Chip text: `12%`, `4.2/16G`, `43°`, `↓1.2M ↑430k`, `16d 3h`.
#[must_use]
pub fn chip_text(chip: Chip, snap: &Snapshot, config: &Config, unit: TempUnit) -> String {
    match chip {
        Chip::Cpu => snap.cpu_pct.map_or_else(|| "…".to_owned(), format::pct_text),
        Chip::Ram => capacity_chip(
            snap.ram.as_ref(),
            config.ram_display,
            SizeUnit::Binary,
        ),
        Chip::Swap => capacity_chip(
            snap.swap.as_ref(),
            config.swap_display,
            SizeUnit::Binary,
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
        Chip::Net => snap.net.first().map_or_else(
            || "…".to_owned(),
            |sample| {
                format!(
                    "↓{} ↑{}",
                    format::speed_compact(sample.down_speed),
                    format::speed_compact(sample.up_speed)
                )
            },
        ),
        Chip::Load => snap
            .load
            .as_ref()
            .map_or_else(|| "…".to_owned(), |load| format::load_text(load.one)),
        Chip::Uptime => snap
            .uptime_seconds
            .map_or_else(|| "…".to_owned(), format::uptime_text),
    }
}

fn capacity_chip(
    info: Option<&RamInfo>,
    display: DisplayMode,
    style: SizeUnit,
) -> String {
    let Some(info) = info else {
        return "…".to_owned();
    };
    match display {
        DisplayMode::Percent => format::pct_text(info.pct),
        DisplayMode::Absolute => format!(
            "{}/{}",
            format::format_compact(info.used, style),
            format::format_compact(info.total, style)
        ),
    }
}

/// Fuller chip text for the tooltip.
#[must_use]
pub fn chip_tooltip(chip: Chip, snap: &Snapshot, config: &Config, unit: TempUnit) -> String {
    match chip {
        Chip::Cpu => snap.cpu_pct.map_or_else(
            || "CPU …".to_owned(),
            |pct| format!("CPU {}", format::pct_text(pct)),
        ),
        Chip::Ram => capacity_tooltip(snap.ram.as_ref(), config.ram_display, SizeUnit::Binary, "RAM"),
        Chip::Swap => capacity_tooltip(snap.swap.as_ref(), config.swap_display, SizeUnit::Binary, "Swap"),
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
        Chip::Net => snap.net.first().map_or_else(
            || "Network …".to_owned(),
            |sample| {
                format!(
                    "Down {}, Up {} — {}",
                    format::speed_text(sample.down_speed),
                    format::speed_text(sample.up_speed),
                    sample.name
                )
            },
        ),
        Chip::Load => snap.load.as_ref().map_or_else(
            || "Load …".to_owned(),
            |load| format!(
                "Load {} · {} · {} (1/5/15 min)",
                format::load_text(load.one),
                format::load_text(load.five),
                format::load_text(load.fifteen)
            ),
        ),
        Chip::Uptime => snap
            .uptime_seconds
            .map_or_else(|| "Uptime …".to_owned(), |seconds| format!(
                "Uptime {}",
                format::uptime_text(seconds)
            )),
    }
}

fn capacity_tooltip(
    info: Option<&RamInfo>,
    display: DisplayMode,
    style: SizeUnit,
    label: &str,
) -> String {
    let Some(info) = info else {
        return format!("{label} …");
    };
    match display {
        DisplayMode::Absolute => format!(
            "{label} {} of {}",
            format::format_full(info.used, style),
            format::format_full(info.total, style)
        ),
        DisplayMode::Percent => format!("{label} {}", format::pct_text(info.pct)),
    }
}

/// Warning/error coloring for one chip: readings that run high.
#[must_use]
pub fn chip_severity(chip: Chip, snap: &Snapshot, config: &Config) -> Severity {
    let level = |pct: Option<f64>| match pct {
        Some(pct) if pct >= USAGE_ERROR_PCT => Severity::Error,
        Some(pct) if pct >= USAGE_WARN_PCT => Severity::Warning,
        _ => Severity::Info,
    };
    match chip {
        Chip::Cpu => level(snap.cpu_pct),
        Chip::Ram => level(snap.ram.as_ref().map(|ram| ram.pct)),
        Chip::Swap => level(snap.swap.as_ref().map(|swap| swap.pct)),
        Chip::Disk => level(
            snap.disks
                .first()
                .map(|disk| format::usage_pct(disk.used, disk.total)),
        ),
        Chip::Temp => match snap
            .temps
            .first()
            .map(|reading| temp_tone(reading, config.temp_warn_c, config.temp_error_c))
        {
            None | Some(Tone::None) => Severity::Info,
            Some(Tone::Warning) => Severity::Warning,
            Some(Tone::Error) => Severity::Error,
        },
        Chip::Net | Chip::Load | Chip::Uptime => Severity::Info,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::{LoadInfo, NetSample};

    fn fixture() -> Snapshot {
        Snapshot {
            cpu_pct: Some(12.0),
            ram: crate::snapshot::usage_info(17_179_869_184, 12_344_554_992),
            swap: crate::snapshot::usage_info(8_589_934_592, 6_442_450_944),
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
            net: vec![NetSample {
                name: "eno1".to_owned(),
                down_speed: 1_230_000.0,
                up_speed: 430_000.0,
                down_total: 4_201_881_600,
                up_total: 1_105_856_000,
            }],
            load: Some(LoadInfo {
                one: 0.5812,
                five: 0.42,
                fifteen: 0.38,
            }),
            uptime_seconds: Some(16 * 86400 + 3 * 3600 + 7 * 60),
            cores: vec![10.0, 90.0],
            gpus: vec![GpuReading {
                model: "NVIDIA RTX 4090".to_owned(),
                usage: Some(64.0),
                memory_used: Some(8_107_438_080),
                memory_total: Some(16_635_408_384),
            }],
            system_info: Some(crate::snapshot::SystemInfo {
                kernel: Some("6.12.0".to_owned()),
                os: Some("Fedora Linux".to_owned()),
                hostname: Some("mercury".to_owned()),
            }),
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
        assert_eq!(chip_text(Chip::Swap, &snap, &config, unit), "25%");
        assert_eq!(chip_text(Chip::Disk, &snap, &config, unit), "26%");
        assert_eq!(chip_text(Chip::Temp, &snap, &config, unit), "43°");
        assert_eq!(chip_text(Chip::Net, &snap, &config, unit), "↓1.2M ↑430k");
        assert_eq!(chip_text(Chip::Load, &snap, &config, unit), "0.58");
        assert_eq!(chip_text(Chip::Uptime, &snap, &config, unit), "16d 3h");
        for chip in [
            Chip::Cpu,
            Chip::Ram,
            Chip::Swap,
            Chip::Disk,
            Chip::Temp,
            Chip::Net,
            Chip::Load,
            Chip::Uptime,
        ] {
            assert_eq!(chip_text(chip, &Snapshot::default(), &config, unit), "…");
        }
    }

    #[test]
    fn absolute_mode_swaps_chip_sizes() {
        let snap = fixture();
        let config = Config {
            ram_display: DisplayMode::Absolute,
            swap_display: DisplayMode::Absolute,
            disk_display: DisplayMode::Absolute,
            ..Config::default()
        };
        let unit = TempUnit::Celsius;
        assert_eq!(chip_text(Chip::Ram, &snap, &config, unit), "4.5G/16G");
        assert_eq!(chip_text(Chip::Swap, &snap, &config, unit), "2G/8G");
        assert_eq!(chip_text(Chip::Disk, &snap, &config, unit), "4.5G/17.2G");
        assert_eq!(
            chip_tooltip(Chip::Disk, &snap, &config, unit),
            "Disk 4.51 GB of 17.18 GB"
        );
        assert_eq!(
            chip_tooltip(Chip::Swap, &snap, &config, unit),
            "Swap 2 GiB of 8 GiB"
        );
    }

    #[test]
    fn tooltips_fully_describe_the_reading() {
        let snap = fixture();
        let config = Config::default();
        let unit = TempUnit::Celsius;
        assert_eq!(chip_tooltip(Chip::Cpu, &snap, &config, unit), "CPU 12%");
        assert_eq!(chip_tooltip(Chip::Ram, &snap, &config, unit), "RAM 28%");
        assert_eq!(chip_tooltip(Chip::Swap, &snap, &config, unit), "Swap 25%");
        assert_eq!(chip_tooltip(Chip::Disk, &snap, &config, unit), "Disk 26%");
        assert_eq!(
            chip_tooltip(Chip::Temp, &snap, &config, unit),
            "Temp 43 °C — Package id 0"
        );
        assert_eq!(
            chip_tooltip(Chip::Net, &snap, &config, unit),
            "Down 1.2M/s, Up 430k/s — eno1"
        );
        assert_eq!(
            chip_tooltip(Chip::Load, &snap, &config, unit),
            "Load 0.58 · 0.42 · 0.38 (1/5/15 min)"
        );
        assert_eq!(chip_tooltip(Chip::Uptime, &snap, &config, unit), "Uptime 16d 3h");
    }

    #[test]
    fn severities_follow_the_thresholds() {
        let config = Config::default();
        for chip in [
            Chip::Cpu,
            Chip::Ram,
            Chip::Swap,
            Chip::Disk,
            Chip::Temp,
            Chip::Net,
            Chip::Load,
            Chip::Uptime,
        ] {
            assert_eq!(
                chip_severity(chip, &Snapshot::default(), &config),
                Severity::Info
            );
        }

        let mut snap = fixture();
        snap.cpu_pct = Some(96.0);
        assert_eq!(chip_severity(Chip::Cpu, &snap, &config), Severity::Error);
        snap.cpu_pct = Some(90.0);
        assert_eq!(chip_severity(Chip::Cpu, &snap, &config), Severity::Warning);
        snap.cpu_pct = Some(12.0);

        snap.swap = crate::snapshot::usage_info(1000, 20);
        assert_eq!(chip_severity(Chip::Swap, &snap, &config), Severity::Error);
        snap.swap = crate::snapshot::usage_info(1000, 95);
        assert_eq!(chip_severity(Chip::Swap, &snap, &config), Severity::Warning);

        snap.temps[0].celsius = 76.0;
        assert_eq!(chip_severity(Chip::Temp, &snap, &config), Severity::Warning);
        snap.temps[0].celsius = 86.0;
        assert_eq!(chip_severity(Chip::Temp, &snap, &config), Severity::Error);
    }

    #[test]
    fn tree_holds_a_chip_and_the_popover() {
        let config = Config::default();
        let shared = Shared::default();
        let nodes = tree(&Ok(config), Some(&shared));
        assert_eq!(nodes.len(), 5); // one chip per default widget + popover

        let issues = crate::config::parse(&serde_json::json!({ "interval": 0 }))
            .expect_err("invalid");
        let nodes = tree(&Err(issues), None);
        assert_eq!(nodes.len(), 1); // the error chip alone
    }

    #[test]
    fn every_default_chip_has_an_icon() {
        for chip in Config::default().chips {
            assert!(chip_icon(chip).starts_with("glimpse-monitor-"));
        }
    }
}
