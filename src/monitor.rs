//! One background thread samples sysinfo on the configured cadence and
//! publishes a [`Snapshot`] for the renderer; wake-ups drive the redraw.

use crate::config::Config;
use crate::snapshot::{self, Shared};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use sysinfo::{
    Components, CpuRefreshKind, Disks, Gpus, MemoryRefreshKind, Networks, System,
};

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
    // System::new() starts with an empty CPU list; fill it so per-core usage
    // has slots, then take the first global reading.
    system.refresh_cpu_list(CpuRefreshKind::nothing().with_usage());
    system.refresh_cpu_usage();
    system.refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram().with_swap());

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
    let mut networks = match Networks::new_with_refreshed_list() {
        Ok(networks) => networks,
        Err(error) => {
            eprintln!("cannot list network interfaces: {error}");
            return;
        }
    };
    let mut gpus = match Gpus::new_with_refreshed_list() {
        Ok(gpus) => gpus,
        Err(error) => {
            eprintln!("cannot list GPUs: {error}");
            return;
        }
    };
    let system_info = read_system_info();

    let mut history: Vec<Option<f64>> = Vec::new();
    loop {
        let tick = Instant::now();

        system.refresh_cpu_usage();
        system.refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram().with_swap());

        disks.refresh(false);
        components.refresh(false);
        networks.refresh(false);
        gpus.refresh(false);

        let snapshot = read(
            &Sources {
                system: &system,
                disks: &disks,
                components: &components,
                networks: &networks,
                gpus: &gpus,
                system_info: &system_info,
            },
            &wanted,
            tick.elapsed().as_secs_f64(),
            &history,
        );
        history = snapshot.history.clone();
        shared.publish(snapshot);
        waker.wake();
        std::thread::sleep(interval);
    }
}

fn read_system_info() -> Option<snapshot::SystemInfo> {
    let kernel = System::kernel_version().ok();
    let os = System::long_os_version().ok();
    let hostname = System::host_name().ok();
    if kernel.is_none() && os.is_none() && hostname.is_none() {
        None
    } else {
        Some(snapshot::SystemInfo {
            kernel,
            os,
            hostname,
        })
    }
}

/// The sysinfo handles a tick reads from, borrowed as one group.
struct Sources<'a> {
    system: &'a System,
    disks: &'a Disks,
    components: &'a Components,
    networks: &'a Networks,
    gpus: &'a Gpus,
    system_info: &'a Option<snapshot::SystemInfo>,
}

/// Turns the sysinfo state plus the previous history into the next snapshot.
fn read(
    sources: &Sources<'_>,
    wanted: &[PathBuf],
    elapsed_seconds: f64,
    history: &[Option<f64>],
) -> snapshot::Snapshot {
    let Sources {
        system,
        disks,
        components,
        networks,
        gpus,
        system_info,
    } = sources;
    let (system, disks, components, networks, gpus, system_info) =
        (*system, *disks, *components, *networks, *gpus, *system_info);
    let cpu_pct = {
        let value = f64::from(system.global_cpu_usage());
        (0.0..=100.0).contains(&value).then_some(value)
    };
    let cores: Vec<f64> = system
        .cpus()
        .iter()
        .map(|cpu| f64::from(cpu.usage()).clamp(0.0, 100.0))
        .collect();
    let ram = snapshot::usage_info(system.total_memory(), system.available_memory());
    let swap = snapshot::usage_info(system.total_swap(), system.free_swap());

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

    let raw_ifaces: Vec<snapshot::RawIface> = networks
        .iter()
        .map(|(name, data)| snapshot::RawIface {
            name: name.to_string_lossy().trim().to_owned(),
            down_delta: data.received(),
            up_delta: data.transmitted(),
            down_total: data.total_received(),
            up_total: data.total_transmitted(),
        })
        .collect();
    let net = snapshot::net_samples(&raw_ifaces, elapsed_seconds);

    let load = System::load_average().ok().map(|load| snapshot::LoadInfo {
        one: load.one,
        five: load.five,
        fifteen: load.fifteen,
    });
    let uptime_seconds = System::uptime().ok();

    let gpus: Vec<snapshot::GpuReading> = gpus
        .iter()
        .map(|gpu| snapshot::GpuReading {
            model: gpu
                .model()
                .or_else(|| gpu.vendor())
                .unwrap_or("GPU")
                .trim()
                .to_owned(),
            usage: gpu.usage().map(|value| f64::from(value).clamp(0.0, 100.0)),
            memory_used: gpu.used_memory(),
            memory_total: gpu.total_memory(),
        })
        .filter(|gpu| gpu.usage.is_some() || gpu.memory_total.unwrap_or(0) > 0)
        .collect();

    let mut history = history.to_vec();
    snapshot::push_history(&mut history, cpu_pct);

    snapshot::Snapshot {
        cpu_pct,
        ram,
        swap,
        disks,
        temps,
        net,
        load,
        uptime_seconds,
        cores,
        gpus,
        system_info: system_info.clone(),
        history,
    }
}
