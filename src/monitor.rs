//! One background thread samples sysinfo on the configured cadence and
//! publishes a [`Snapshot`] for the renderer; wake-ups drive the redraw.

use crate::config::Config;
use crate::snapshot::{self, Shared};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use sysinfo::{Components, Disks, MemoryRefreshKind, System};

/// Hands out the shared snapshot and starts the sampling thread.
#[must_use]
pub fn spawn(waker: glimpse_sdk::Waker, config: Config) -> Arc<Shared> {
    let shared = Arc::new(Shared::default());
    let reader = shared.clone();
    if let Err(error) = std::thread::Builder::new()
        .name("glimpse-monitor-sampler".to_owned())
        .spawn(move || sample_loop(waker, config, reader))
    {
        eprintln!("cannot start the sampling thread: {error}");
    }
    shared
}

fn sample_loop(waker: glimpse_sdk::Waker, config: Config, shared: Arc<Shared>) {
    let interval = Duration::from_secs(config.interval_seconds.max(1));
    let wanted: Vec<PathBuf> = config.mounts.iter().map(PathBuf::from).collect();

    // The first CPU reading needs a previous tick to diff against, so sample
    // once before the loop: the first published tick already has percentages.
    let mut system = match System::new() {
        Ok(system) => system,
        Err(error) => {
            eprintln!("cannot open system information: {error}");
            return;
        }
    };
    system.refresh_cpu_usage();
    system.refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());

    let mut disks = match Disks::new_with_refreshed_list() {
        Ok(disks) => disks,
        Err(error) => {
            eprintln!("cannot list disks: {error}");
            return;
        }
    };
    let mut components = match Components::new_with_refreshed_list() {
        Ok(components) => components,
        Err(error) => {
            eprintln!("cannot list temperature sensors: {error}");
            return;
        }
    };

    let mut history: Vec<Option<f64>> = Vec::new();
    loop {
        system.refresh_cpu_usage();
        system.refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());

        disks.refresh(false);
        components.refresh(false);

        let snapshot = read(&system, &disks, &components, &wanted, &history);
        history = snapshot.history.clone();
        shared.publish(snapshot);
        waker.wake();
        std::thread::sleep(interval);
    }
}

/// Turns the sysinfo state plus the previous history into the next snapshot.
fn read(
    system: &System,
    disks: &Disks,
    components: &Components,
    wanted: &[PathBuf],
    history: &[Option<f64>],
) -> snapshot::Snapshot {
    let cpu_pct = {
        let value = f64::from(system.global_cpu_usage());
        (0.0..=100.0).contains(&value).then_some(value)
    };
    let ram = snapshot::ram_info(system.total_memory(), system.available_memory());

    // Keep the configured mount order; skip mounts sysinfo does not list.
    let disks: Vec<snapshot::DiskUsage> = wanted
        .iter()
        .filter_map(|mount| {
            let disk = disks
                .iter()
                .find(|disk| disk.mount_point() == Path::new(mount.as_os_str()))?;
            let total = disk.total_space();
            let used = total.saturating_sub(disk.available_space());
            (total > 0).then(|| snapshot::DiskUsage {
                mount: mount.to_string_lossy().into_owned(),
                used,
                total,
            })
        })
        .collect();

    let mut temps: Vec<snapshot::TempReading> = components
        .iter()
        .filter_map(|component| {
            let celsius = component.temperature()?;
            let label = component.label().trim().to_owned();
            let label = if label.is_empty() {
                component.id().unwrap_or_default().trim().to_owned()
            } else {
                label
            };
            if label.is_empty() {
                return None;
            }
            Some(snapshot::TempReading {
                label,
                celsius,
                critical_c: component.critical(),
                peak_c: component.max(),
            })
        })
        .collect();
    // Hottest first, keeping the sensor order for equal readings.
    temps.sort_by(|a, b| {
        b.celsius
            .partial_cmp(&a.celsius)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut history = history.to_vec();
    snapshot::push_history(&mut history, cpu_pct);

    snapshot::Snapshot {
        cpu_pct,
        ram,
        disks,
        temps,
        history,
    }
}
