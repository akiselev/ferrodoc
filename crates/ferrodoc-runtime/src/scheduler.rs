//! Deterministic worker and memory leases with bounded backpressure.

use std::{
    collections::BTreeMap,
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};

use ferrodoc_core::{Bytes, DeviceId, Estimate, Sha256Digest};
use ferrodoc_engine_api::CancellationToken;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Scheduler hard capacities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SchedulerConfig {
    /// Maximum simultaneous blocking engine calls.
    pub cpu_workers: u32,
    /// Host RAM available to scheduled engine work.
    pub ram_budget: Bytes,
    /// Per-device VRAM capacities.
    #[serde(default)]
    pub device_budgets: BTreeMap<DeviceId, Bytes>,
}

/// Model memory retained after a request completes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WarmResidency {
    /// Immutable model digest.
    pub model_digest: Sha256Digest,
    /// Physical device holding the model.
    pub device: DeviceId,
    /// Retained VRAM.
    pub bytes: Bytes,
}

/// Resources requested for one blocking engine operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LeaseRequest {
    /// Estimated peak host RAM.
    pub ram: Estimate<Bytes>,
    /// Optional device and estimated peak VRAM.
    pub device: Option<(DeviceId, Estimate<Bytes>)>,
    /// Optional warm model state established after successful execution.
    pub retain_warm: Option<WarmResidency>,
    /// Explicit permission to reserve an entire budget for an unknown estimate.
    pub guard_unknown: bool,
}

/// Observed peaks associated with a lease.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LeaseMeasurement {
    /// Highest observed host RAM attributable to the operation.
    pub peak_ram: Option<Bytes>,
    /// Highest observed device VRAM attributable to the operation.
    pub peak_vram: Option<Bytes>,
}

/// Stable scheduler failure.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SchedulerError {
    /// Configuration contains a zero worker count or budget inconsistency.
    #[error("invalid scheduler configuration: {0}")]
    InvalidConfiguration(String),
    /// Unknown resource estimate was not explicitly admitted as guarded execution.
    #[error("unknown {0} estimate requires guarded execution")]
    UnknownEstimate(&'static str),
    /// Requested resource exceeds total configured capacity.
    #[error("requested {resource} {requested} exceeds configured budget {budget}")]
    ExceedsBudget {
        /// Resource class.
        resource: &'static str,
        /// Requested bytes.
        requested: Bytes,
        /// Configured bytes.
        budget: Bytes,
    },
    /// Device has no configured capacity.
    #[error("device {0} has no configured resource budget")]
    UnknownDevice(DeviceId),
    /// Capacity exists but is currently leased.
    #[error("scheduler backpressure: resources are currently leased")]
    Backpressure,
    /// Caller cancellation was observed while waiting or executing.
    #[error("scheduler request cancelled")]
    Cancelled,
    /// Admission deadline elapsed.
    #[error("scheduler admission deadline exceeded")]
    DeadlineExceeded,
    /// Observed use exceeded the granted reservation.
    #[error("observed {resource} {observed} exceeded reservation {reserved}")]
    ObservationExceeded {
        /// Resource class.
        resource: &'static str,
        /// Observed peak.
        observed: Bytes,
        /// Granted reservation.
        reserved: Bytes,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct WarmKey {
    model_digest: Sha256Digest,
    device: DeviceId,
}

#[derive(Debug, Default)]
struct State {
    workers: u32,
    ram: u64,
    device: BTreeMap<DeviceId, u64>,
    warm: BTreeMap<WarmKey, u64>,
}

#[derive(Debug)]
struct Shared {
    config: SchedulerConfig,
    state: Mutex<State>,
    changed: Condvar,
}

/// Cloneable scheduler coordinating leases across worker threads.
#[derive(Debug, Clone)]
pub struct Scheduler(Arc<Shared>);

impl Scheduler {
    /// Creates a scheduler after validating its hard capacities.
    pub fn new(config: SchedulerConfig) -> Result<Self, SchedulerError> {
        if config.cpu_workers == 0 {
            return Err(SchedulerError::InvalidConfiguration(
                "cpu_workers must be nonzero".into(),
            ));
        }
        Ok(Self(Arc::new(Shared {
            config,
            state: Mutex::new(State::default()),
            changed: Condvar::new(),
        })))
    }

    /// Attempts admission once without waiting.
    pub fn try_acquire(
        &self,
        request: LeaseRequest,
        cancellation: CancellationToken,
    ) -> Result<ResourceLease, SchedulerError> {
        if cancellation.is_cancelled() {
            return Err(SchedulerError::Cancelled);
        }
        let normalized = self.normalize(request)?;
        let mut state = self.0.state.lock().expect("scheduler lock poisoned");
        if !fits(&self.0.config, &state, &normalized) {
            return Err(SchedulerError::Backpressure);
        }
        grant(&mut state, &normalized);
        Ok(ResourceLease {
            shared: Arc::clone(&self.0),
            request: normalized,
            cancellation,
            measurement: LeaseMeasurement::default(),
            successful: false,
        })
    }

    /// Waits with backpressure until resources, cancellation, or deadline wins.
    pub fn acquire(
        &self,
        request: LeaseRequest,
        cancellation: CancellationToken,
        deadline: Option<Instant>,
    ) -> Result<ResourceLease, SchedulerError> {
        let normalized = self.normalize(request)?;
        let mut state = self.0.state.lock().expect("scheduler lock poisoned");
        loop {
            if cancellation.is_cancelled() {
                return Err(SchedulerError::Cancelled);
            }
            if deadline.is_some_and(|value| Instant::now() >= value) {
                return Err(SchedulerError::DeadlineExceeded);
            }
            if fits(&self.0.config, &state, &normalized) {
                grant(&mut state, &normalized);
                return Ok(ResourceLease {
                    shared: Arc::clone(&self.0),
                    request: normalized,
                    cancellation,
                    measurement: LeaseMeasurement::default(),
                    successful: false,
                });
            }
            let wait = deadline.map_or(Duration::from_millis(20), |value| {
                value
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(20))
            });
            let (next, _) = self
                .0
                .changed
                .wait_timeout(state, wait)
                .expect("scheduler lock poisoned");
            state = next;
        }
    }

    /// Evicts one warm model reservation, returning whether it existed.
    pub fn evict_warm(&self, model_digest: Sha256Digest, device: &DeviceId) -> bool {
        let key = WarmKey {
            model_digest,
            device: device.clone(),
        };
        let mut state = self.0.state.lock().expect("scheduler lock poisoned");
        let Some(bytes) = state.warm.remove(&key) else {
            return false;
        };
        subtract_device(&mut state, device, bytes);
        self.0.changed.notify_all();
        true
    }

    /// Returns current deterministic lease accounting for diagnostics.
    pub fn snapshot(&self) -> SchedulerSnapshot {
        let state = self.0.state.lock().expect("scheduler lock poisoned");
        SchedulerSnapshot {
            active_workers: state.workers,
            reserved_ram: Bytes::new(state.ram),
            reserved_devices: state
                .device
                .iter()
                .map(|(device, bytes)| (device.clone(), Bytes::new(*bytes)))
                .collect(),
            warm_models: state.warm.len() as u64,
        }
    }

    fn normalize(&self, request: LeaseRequest) -> Result<NormalizedRequest, SchedulerError> {
        let ram = normalize_estimate(
            "RAM",
            &request.ram,
            self.0.config.ram_budget,
            request.guard_unknown,
        )?;
        let device = request
            .device
            .map(|(device, estimate)| {
                let budget = self
                    .0
                    .config
                    .device_budgets
                    .get(&device)
                    .copied()
                    .ok_or_else(|| SchedulerError::UnknownDevice(device.clone()))?;
                let bytes = normalize_estimate("VRAM", &estimate, budget, request.guard_unknown)?;
                Ok((device, bytes))
            })
            .transpose()?;
        if let Some(warm) = &request.retain_warm {
            let Some((device, peak)) = &device else {
                return Err(SchedulerError::InvalidConfiguration(
                    "warm residency requires a device lease".into(),
                ));
            };
            if warm.device != *device || warm.bytes > *peak {
                return Err(SchedulerError::InvalidConfiguration(
                    "warm residency must use the leased device and not exceed peak VRAM".into(),
                ));
            }
        }
        Ok(NormalizedRequest {
            ram,
            device,
            retain_warm: request.retain_warm,
        })
    }
}

/// Current scheduler accounting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SchedulerSnapshot {
    /// Active worker slots.
    pub active_workers: u32,
    /// Active host RAM reservations.
    pub reserved_ram: Bytes,
    /// Active plus warm device reservations.
    pub reserved_devices: BTreeMap<DeviceId, Bytes>,
    /// Count of warm model/device pairs.
    pub warm_models: u64,
}

#[derive(Debug)]
struct NormalizedRequest {
    ram: Bytes,
    device: Option<(DeviceId, Bytes)>,
    retain_warm: Option<WarmResidency>,
}

/// Granted resources released automatically on drop.
#[derive(Debug)]
pub struct ResourceLease {
    shared: Arc<Shared>,
    request: NormalizedRequest,
    cancellation: CancellationToken,
    measurement: LeaseMeasurement,
    successful: bool,
}

impl ResourceLease {
    /// Records actual peaks and cancels execution if a reservation is exceeded.
    pub fn observe(
        &mut self,
        peak_ram: Option<Bytes>,
        peak_vram: Option<Bytes>,
    ) -> Result<(), SchedulerError> {
        if self.cancellation.is_cancelled() {
            return Err(SchedulerError::Cancelled);
        }
        if let Some(observed) = peak_ram {
            self.measurement.peak_ram = Some(maximum(self.measurement.peak_ram, observed));
            if observed > self.request.ram {
                self.cancellation.cancel();
                return Err(SchedulerError::ObservationExceeded {
                    resource: "RAM",
                    observed,
                    reserved: self.request.ram,
                });
            }
        }
        if let Some(observed) = peak_vram {
            self.measurement.peak_vram = Some(maximum(self.measurement.peak_vram, observed));
            let reserved = self
                .request
                .device
                .as_ref()
                .map_or(Bytes::new(0), |(_, bytes)| *bytes);
            if observed > reserved {
                self.cancellation.cancel();
                return Err(SchedulerError::ObservationExceeded {
                    resource: "VRAM",
                    observed,
                    reserved,
                });
            }
        }
        Ok(())
    }

    /// Marks execution successful, permitting requested warm residency on drop.
    pub fn complete(&mut self) {
        self.successful = true;
    }

    /// Returns observations recorded so far.
    pub const fn measurement(&self) -> LeaseMeasurement {
        self.measurement
    }
}

impl Drop for ResourceLease {
    fn drop(&mut self) {
        let mut state = self.shared.state.lock().expect("scheduler lock poisoned");
        state.workers = state.workers.saturating_sub(1);
        state.ram = state.ram.saturating_sub(self.request.ram.get());
        if let Some((device, bytes)) = &self.request.device {
            subtract_device(&mut state, device, bytes.get());
        }
        if self.successful
            && let Some(warm) = &self.request.retain_warm
        {
            let key = WarmKey {
                model_digest: warm.model_digest,
                device: warm.device.clone(),
            };
            if let std::collections::btree_map::Entry::Vacant(entry) = state.warm.entry(key) {
                entry.insert(warm.bytes.get());
                *state.device.entry(warm.device.clone()).or_default() += warm.bytes.get();
            }
        }
        self.shared.changed.notify_all();
    }
}

fn normalize_estimate(
    resource: &'static str,
    estimate: &Estimate<Bytes>,
    budget: Bytes,
    guard_unknown: bool,
) -> Result<Bytes, SchedulerError> {
    let requested = match estimate {
        Estimate::Known(value) => *value,
        Estimate::Unknown if guard_unknown => budget,
        Estimate::Unknown => return Err(SchedulerError::UnknownEstimate(resource)),
    };
    if requested > budget {
        return Err(SchedulerError::ExceedsBudget {
            resource,
            requested,
            budget,
        });
    }
    Ok(requested)
}

fn fits(config: &SchedulerConfig, state: &State, request: &NormalizedRequest) -> bool {
    if state.workers >= config.cpu_workers
        || state.ram.saturating_add(request.ram.get()) > config.ram_budget.get()
    {
        return false;
    }
    request.device.as_ref().is_none_or(|(device, bytes)| {
        let used = state.device.get(device).copied().unwrap_or(0);
        let budget = config
            .device_budgets
            .get(device)
            .map_or(0, |value| value.get());
        used.saturating_add(bytes.get()) <= budget
    })
}

fn grant(state: &mut State, request: &NormalizedRequest) {
    state.workers += 1;
    state.ram += request.ram.get();
    if let Some((device, bytes)) = &request.device {
        *state.device.entry(device.clone()).or_default() += bytes.get();
    }
}

fn subtract_device(state: &mut State, device: &DeviceId, bytes: u64) {
    let used = state.device.entry(device.clone()).or_default();
    *used = used.saturating_sub(bytes);
    if *used == 0 {
        state.device.remove(device);
    }
}

fn maximum(current: Option<Bytes>, observed: Bytes) -> Bytes {
    current.map_or(observed, |value| value.max(observed))
}

#[cfg(test)]
mod tests {
    use std::{thread, time::Duration};

    use ferrodoc_core::DeviceKind;

    use super::*;

    fn cuda() -> DeviceId {
        DeviceId::new(DeviceKind::Cuda, Some(0)).unwrap()
    }

    fn scheduler() -> Scheduler {
        Scheduler::new(SchedulerConfig {
            cpu_workers: 2,
            ram_budget: Bytes::new(4 * Bytes::GIB),
            device_budgets: BTreeMap::from([(cuda(), Bytes::new(2 * Bytes::GIB))]),
        })
        .unwrap()
    }

    fn request(vram: Estimate<Bytes>) -> LeaseRequest {
        LeaseRequest {
            ram: Estimate::Known(Bytes::new(Bytes::GIB)),
            device: Some((cuda(), vram)),
            retain_warm: None,
            guard_unknown: false,
        }
    }

    #[test]
    fn overlapping_device_leases_never_exceed_budget() {
        let scheduler = scheduler();
        let first = scheduler
            .try_acquire(
                request(Estimate::Known(Bytes::new(1536 * Bytes::MIB))),
                CancellationToken::default(),
            )
            .unwrap();
        assert!(matches!(
            scheduler.try_acquire(
                request(Estimate::Known(Bytes::new(Bytes::GIB))),
                CancellationToken::default(),
            ),
            Err(SchedulerError::Backpressure)
        ));
        drop(first);
        assert!(
            scheduler
                .try_acquire(
                    request(Estimate::Known(Bytes::new(Bytes::GIB))),
                    CancellationToken::default(),
                )
                .is_ok()
        );
    }

    #[test]
    fn guarded_unknown_reserves_the_entire_device() {
        let scheduler = scheduler();
        let mut guarded = request(Estimate::Unknown);
        guarded.guard_unknown = true;
        let lease = scheduler
            .try_acquire(guarded, CancellationToken::default())
            .unwrap();
        assert_eq!(
            scheduler.snapshot().reserved_devices[&cuda()],
            Bytes::new(2 * Bytes::GIB)
        );
        assert!(matches!(
            scheduler.try_acquire(
                request(Estimate::Known(Bytes::new(1))),
                CancellationToken::default(),
            ),
            Err(SchedulerError::Backpressure)
        ));
        drop(lease);
    }

    #[test]
    fn waiting_acquisition_observes_cancellation() {
        let scheduler = Scheduler::new(SchedulerConfig {
            cpu_workers: 1,
            ram_budget: Bytes::new(Bytes::GIB),
            device_budgets: BTreeMap::new(),
        })
        .unwrap();
        let held = scheduler
            .try_acquire(
                LeaseRequest {
                    ram: Estimate::Known(Bytes::new(Bytes::GIB)),
                    device: None,
                    retain_warm: None,
                    guard_unknown: false,
                },
                CancellationToken::default(),
            )
            .unwrap();
        let cancellation = CancellationToken::default();
        let worker_scheduler = scheduler.clone();
        let worker_cancellation = cancellation.clone();
        let worker = thread::spawn(move || {
            worker_scheduler.acquire(
                LeaseRequest {
                    ram: Estimate::Known(Bytes::new(1)),
                    device: None,
                    retain_warm: None,
                    guard_unknown: false,
                },
                worker_cancellation,
                Some(Instant::now() + Duration::from_secs(1)),
            )
        });
        thread::sleep(Duration::from_millis(30));
        cancellation.cancel();
        assert!(matches!(
            worker.join().unwrap(),
            Err(SchedulerError::Cancelled)
        ));
        drop(held);
    }

    #[test]
    fn warm_residency_is_counted_and_evictable() {
        let scheduler = scheduler();
        let digest = Sha256Digest::of_bytes(b"model");
        let mut request = request(Estimate::Known(Bytes::new(Bytes::GIB)));
        request.retain_warm = Some(WarmResidency {
            model_digest: digest,
            device: cuda(),
            bytes: Bytes::new(512 * Bytes::MIB),
        });
        let mut lease = scheduler
            .try_acquire(request, CancellationToken::default())
            .unwrap();
        lease.complete();
        drop(lease);
        assert_eq!(scheduler.snapshot().warm_models, 1);
        assert!(scheduler.evict_warm(digest, &cuda()));
        assert_eq!(scheduler.snapshot().warm_models, 0);
    }

    #[test]
    fn observation_beyond_reservation_cancels_execution() {
        let scheduler = scheduler();
        let cancellation = CancellationToken::default();
        let mut lease = scheduler
            .try_acquire(
                request(Estimate::Known(Bytes::new(Bytes::GIB))),
                cancellation.clone(),
            )
            .unwrap();
        assert!(matches!(
            lease.observe(None, Some(Bytes::new(Bytes::GIB + 1))),
            Err(SchedulerError::ObservationExceeded { .. })
        ));
        assert!(cancellation.is_cancelled());
    }
}
