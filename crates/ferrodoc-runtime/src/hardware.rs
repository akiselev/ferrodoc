//! Host hardware inventory with explicit provenance and unknown values.

use std::{collections::BTreeSet, fs};

use ferrodoc_core::{Bytes, Estimate, EstimateConfidence, EstimateSource};
use ferrodoc_engine_api::HardwareInventory;

/// Collects the current host inventory without failing when a source is unavailable.
pub fn inventory() -> HardwareInventory {
    let logical_cpus = std::thread::available_parallelism()
        .ok()
        .and_then(|count| u32::try_from(count.get()).ok())
        .map_or(Estimate::Unknown, Estimate::Known);
    let cpuinfo = fs::read_to_string("/proc/cpuinfo").ok();
    let physical_cpus = cpuinfo
        .as_deref()
        .and_then(parse_linux_physical_cpus)
        .map_or(Estimate::Unknown, Estimate::Known);
    let cpu_source = if logical_cpus.is_unknown() && physical_cpus.is_unknown() {
        Estimate::Unknown
    } else {
        Estimate::Known(EstimateSource {
            confidence: EstimateConfidence::Measured,
            method: if cpuinfo.is_some() {
                "os.parallelism+linux.proc.cpuinfo".into()
            } else {
                "os.parallelism".into()
            },
        })
    };
    let (ram_total, ram_available, ram_source) = fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|text| parse_linux_meminfo(&text))
        .map_or(
            (Estimate::Unknown, Estimate::Unknown, Estimate::Unknown),
            |(total, available)| {
                (
                    Estimate::Known(total),
                    Estimate::Known(available),
                    Estimate::Known(EstimateSource {
                        confidence: EstimateConfidence::Measured,
                        method: "linux.proc.meminfo".into(),
                    }),
                )
            },
        );
    HardwareInventory {
        logical_cpus,
        physical_cpus,
        cpu_source,
        ram_total,
        ram_available,
        ram_source,
        devices: nvidia_devices(),
    }
}

fn parse_linux_meminfo(input: &str) -> Option<(Bytes, Bytes)> {
    fn value(input: &str, key: &str) -> Option<Bytes> {
        let line = input.lines().find(|line| line.starts_with(key))?;
        let mut fields = line.split_ascii_whitespace();
        let _ = fields.next()?;
        let kib = fields.next()?.parse::<u64>().ok()?;
        if fields.next()? != "kB" {
            return None;
        }
        kib.checked_mul(Bytes::KIB).map(Bytes::new)
    }
    Some((value(input, "MemTotal:")?, value(input, "MemAvailable:")?))
}

fn parse_linux_physical_cpus(input: &str) -> Option<u32> {
    let mut cores = BTreeSet::new();
    let mut physical = None;
    let mut core = None;
    for line in input.lines().chain(std::iter::once("")) {
        if line.trim().is_empty() {
            if let (Some(package), Some(core_id)) = (physical.take(), core.take()) {
                cores.insert((package, core_id));
            }
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        match key.trim() {
            "physical id" => physical = value.trim().parse::<u32>().ok(),
            "core id" => core = value.trim().parse::<u32>().ok(),
            _ => {}
        }
    }
    u32::try_from(cores.len()).ok().filter(|count| *count > 0)
}

#[cfg(not(feature = "nvml"))]
fn nvidia_devices() -> Vec<ferrodoc_engine_api::DeviceInventory> {
    Vec::new()
}

#[cfg(feature = "nvml")]
fn nvidia_devices() -> Vec<ferrodoc_engine_api::DeviceInventory> {
    use std::collections::BTreeMap;

    use ferrodoc_core::{DeviceId, DeviceKind};
    use ferrodoc_engine_api::DeviceInventory;
    use nvml_wrapper::Nvml;

    let Ok(nvml) = Nvml::init() else {
        return Vec::new();
    };
    let Ok(count) = nvml.device_count() else {
        return Vec::new();
    };
    (0..count)
        .filter_map(|index| {
            let device = nvml.device_by_index(index).ok()?;
            let memory = device.memory_info().ok()?;
            let mut metadata = BTreeMap::new();
            if let Ok(name) = device.name() {
                metadata.insert("name".into(), name);
            }
            if let Ok(uuid) = device.uuid() {
                metadata.insert("uuid".into(), uuid);
            }
            Some(DeviceInventory {
                id: DeviceId::new(DeviceKind::Cuda, Some(index)).ok()?,
                memory_total: Estimate::Known(Bytes::new(memory.total)),
                memory_available: Estimate::Known(Bytes::new(memory.free)),
                metadata,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_meminfo_fixture_without_host_assumptions() {
        let parsed = parse_linux_meminfo(
            "MemTotal:       16384 kB\nMemFree: 1 kB\nMemAvailable:    4096 kB\n",
        )
        .unwrap();
        assert_eq!(parsed.0, Bytes::new(16 * Bytes::MIB));
        assert_eq!(parsed.1, Bytes::new(4 * Bytes::MIB));
        assert!(parse_linux_meminfo("MemTotal: nope kB").is_none());
    }

    #[test]
    fn parses_physical_topology_fixture() {
        let input = "physical id: 0\ncore id: 0\n\nphysical id: 0\ncore id: 1\n\nphysical id: 1\ncore id: 0\n";
        assert_eq!(parse_linux_physical_cpus(input), Some(3));
        assert_eq!(parse_linux_physical_cpus("processor: 0\n"), None);
    }
}
