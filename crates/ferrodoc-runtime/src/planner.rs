//! Explainable candidate filtering and deterministic profile selection.

use std::{cmp::Ordering, collections::BTreeSet};

use ferrodoc_core::{
    Bytes, Capability, DeviceKind, Estimate, MicroUsd, Millis, ModelId, PlacementPolicy, Profile,
    Sha256Digest,
};
use ferrodoc_engine_api::{EngineCandidate, EngineDescriptor, HardwareInventory, NetworkUse};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Model readiness attached to one engine candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ModelAvailability {
    /// Candidate does not require a model.
    NotRequired,
    /// The exact immutable model is installed.
    Available {
        /// Installed model ID.
        id: ModelId,
        /// Digest participating in cache identity.
        digest: Sha256Digest,
    },
    /// A required model is absent.
    Missing {
        /// Missing model ID.
        id: ModelId,
    },
}

/// Descriptor, estimate, and model state considered as one candidate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CandidateInput {
    /// Engine-level capabilities and policy facts.
    pub descriptor: EngineDescriptor,
    /// Concrete backend/device placement and estimate.
    pub candidate: EngineCandidate,
    /// Model availability for this placement.
    pub model: ModelAvailability,
}

/// Stable planner reason code.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum ReasonCode {
    /// Candidate passed all hard constraints.
    Accepted,
    /// Engine does not provide the required semantic capability.
    CapabilityUnsupported,
    /// Candidate identity does not match its descriptor.
    DescriptorMismatch,
    /// Backend/device pair is not declared compatible.
    DeviceIncompatible,
    /// Placement policy excludes the device.
    PlacementRejected,
    /// Candidate is not in the explicit allow-list.
    EngineNotAllowed,
    /// Required model is absent.
    ModelUnavailable,
    /// Offline policy cannot prove network-free execution.
    OfflineViolation,
    /// Private policy cannot prove content remains local.
    PrivacyViolation,
    /// A hard RAM estimate is unknown.
    RamUnknown,
    /// Peak RAM exceeds the applicable hard bound.
    RamBudgetExceeded,
    /// A hard VRAM estimate is unknown.
    VramUnknown,
    /// Peak VRAM exceeds the applicable hard bound.
    VramBudgetExceeded,
    /// A hard remote-cost estimate is unknown.
    CostUnknown,
    /// Remote cost exceeds the hard bound.
    CostBudgetExceeded,
    /// A hard latency estimate is unknown.
    LatencyUnknown,
    /// Estimated latency exceeds the deadline.
    DeadlineExceeded,
}

/// Machine-readable code plus a stable human explanation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlannerReason {
    /// Stable code suitable for automation.
    pub code: ReasonCode,
    /// Human-readable explanation with relevant values.
    pub explanation: String,
}

/// Result for one enumerated candidate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CandidateDecision {
    /// Original candidate.
    pub input: CandidateInput,
    /// True only when every hard constraint passed.
    pub accepted: bool,
    /// Deterministically ordered reasons.
    pub reasons: Vec<PlannerReason>,
}

/// Planner hard constraints and soft profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlannerPolicy {
    /// Soft objective bundle, with documented hard restrictions for some profiles.
    pub profile: Profile,
    /// Required semantic capability.
    pub capability: Capability,
    /// Device placement constraint.
    pub placement: PlacementPolicy,
    /// Empty means every discovered engine is allowed.
    #[serde(default)]
    pub allowed_engines: BTreeSet<String>,
    /// Maximum peak host RAM.
    pub max_ram: Option<Bytes>,
    /// Maximum peak device memory.
    pub max_vram: Option<Bytes>,
    /// Maximum remote monetary cost.
    pub max_remote_cost: Option<MicroUsd>,
    /// Maximum estimated stage duration.
    pub deadline: Option<Millis>,
    /// Require a provably network-free candidate.
    pub offline: bool,
    /// Require a candidate that cannot disclose document content.
    pub private: bool,
    /// Explicit opt-in allowing unknown values to pass hard numeric limits.
    pub allow_unknown_hard_estimates: bool,
}

impl PlannerPolicy {
    /// Creates policy defaults for one built-in profile.
    pub fn for_profile(profile: Profile, capability: Capability) -> Self {
        let mut policy = Self {
            profile,
            capability,
            placement: PlacementPolicy::Auto,
            allowed_engines: BTreeSet::new(),
            max_ram: None,
            max_vram: None,
            max_remote_cost: None,
            deadline: None,
            offline: false,
            private: false,
            allow_unknown_hard_estimates: false,
        };
        match profile {
            Profile::Cpu => policy.placement = PlacementPolicy::CpuOnly,
            Profile::LowVram => policy.max_vram = Some(Bytes::new(2 * Bytes::GIB)),
            Profile::Offline => policy.offline = true,
            Profile::Private => policy.private = true,
            Profile::Fast | Profile::Balanced | Profile::Accurate | Profile::Cheap => {}
        }
        policy
    }
}

/// Complete candidate audit and selected Pareto preference, if any.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlanningReport {
    /// Policy applied to every candidate.
    pub policy: PlannerPolicy,
    /// Candidate decisions in stable input-independent order.
    pub decisions: Vec<CandidateDecision>,
    /// Selected accepted candidate; absent when every candidate was rejected.
    pub selected: Option<EngineCandidate>,
}

/// Applies hard constraints and then a deterministic profile preference.
pub fn plan(
    policy: PlannerPolicy,
    inventory: &HardwareInventory,
    candidates: impl IntoIterator<Item = CandidateInput>,
) -> PlanningReport {
    let mut decisions: Vec<_> = candidates
        .into_iter()
        .map(|input| decide(&policy, inventory, input))
        .collect();
    decisions.sort_by(|left, right| {
        candidate_identity(&left.input).cmp(&candidate_identity(&right.input))
    });
    let selected = decisions
        .iter()
        .filter(|decision| decision.accepted)
        .min_by(|left, right| {
            compare_preferred_device(&policy.placement, &left.input, &right.input)
                .then_with(|| compare_profile(policy.profile, &left.input, &right.input))
        })
        .map(|decision| decision.input.candidate.clone());
    PlanningReport {
        policy,
        decisions,
        selected,
    }
}

fn decide(
    policy: &PlannerPolicy,
    inventory: &HardwareInventory,
    input: CandidateInput,
) -> CandidateDecision {
    let mut reasons = Vec::new();
    let descriptor = &input.descriptor;
    let candidate = &input.candidate;
    if !descriptor.capabilities.contains(&policy.capability) {
        reject(
            &mut reasons,
            ReasonCode::CapabilityUnsupported,
            format!(
                "engine {} does not declare {}",
                descriptor.id, policy.capability
            ),
        );
    }
    if descriptor.id != candidate.engine_id {
        reject(
            &mut reasons,
            ReasonCode::DescriptorMismatch,
            "candidate engine ID differs from its descriptor".into(),
        );
    }
    let compatible = descriptor.compatibility.iter().any(|compatibility| {
        compatibility.backend == candidate.backend
            && compatibility.devices.contains(&candidate.device.kind())
    });
    if !compatible {
        reject(
            &mut reasons,
            ReasonCode::DeviceIncompatible,
            format!(
                "backend {} on {} is not declared compatible",
                candidate.backend, candidate.device
            ),
        );
    }
    if candidate.device.kind() != DeviceKind::Cpu
        && !inventory
            .devices
            .iter()
            .any(|device| device.id == candidate.device)
    {
        reject(
            &mut reasons,
            ReasonCode::DeviceIncompatible,
            format!(
                "device {} is absent from hardware inventory",
                candidate.device
            ),
        );
    }
    if !placement_matches(&policy.placement, candidate) {
        reject(
            &mut reasons,
            ReasonCode::PlacementRejected,
            format!("device {} violates placement policy", candidate.device),
        );
    }
    if !policy.allowed_engines.is_empty() && !policy.allowed_engines.contains(&descriptor.id) {
        reject(
            &mut reasons,
            ReasonCode::EngineNotAllowed,
            format!("engine {} is absent from the allow-list", descriptor.id),
        );
    }
    if let ModelAvailability::Missing { id } = &input.model {
        reject(
            &mut reasons,
            ReasonCode::ModelUnavailable,
            format!("required model {id} is not installed"),
        );
    }
    if policy.offline && descriptor.network_use != NetworkUse::None {
        reject(
            &mut reasons,
            ReasonCode::OfflineViolation,
            "offline policy requires network_use=none".into(),
        );
    }
    if policy.private && descriptor.network_use != NetworkUse::None {
        reject(
            &mut reasons,
            ReasonCode::PrivacyViolation,
            "private policy requires network_use=none".into(),
        );
    }
    check_limit(
        &mut reasons,
        "RAM",
        &candidate.resources.peak_ram,
        effective_ram_limit(policy, inventory),
        policy.allow_unknown_hard_estimates,
        ReasonCode::RamUnknown,
        ReasonCode::RamBudgetExceeded,
        Bytes::get,
    );
    check_limit(
        &mut reasons,
        "VRAM",
        &candidate.resources.peak_vram,
        effective_vram_limit(policy, inventory, candidate),
        policy.allow_unknown_hard_estimates,
        ReasonCode::VramUnknown,
        ReasonCode::VramBudgetExceeded,
        Bytes::get,
    );
    check_limit(
        &mut reasons,
        "remote cost",
        &candidate.resources.remote_cost,
        policy.max_remote_cost,
        policy.allow_unknown_hard_estimates,
        ReasonCode::CostUnknown,
        ReasonCode::CostBudgetExceeded,
        MicroUsd::get,
    );
    check_limit(
        &mut reasons,
        "latency",
        &candidate.resources.latency,
        policy.deadline,
        policy.allow_unknown_hard_estimates,
        ReasonCode::LatencyUnknown,
        ReasonCode::DeadlineExceeded,
        Millis::get,
    );
    let accepted = reasons.is_empty();
    if accepted {
        reasons.push(PlannerReason {
            code: ReasonCode::Accepted,
            explanation: "candidate satisfies every hard constraint".into(),
        });
    }
    CandidateDecision {
        input,
        accepted,
        reasons,
    }
}

fn effective_ram_limit(policy: &PlannerPolicy, inventory: &HardwareInventory) -> Option<Bytes> {
    minimum(policy.max_ram, inventory.ram_available.known().copied())
}

fn effective_vram_limit(
    policy: &PlannerPolicy,
    inventory: &HardwareInventory,
    candidate: &EngineCandidate,
) -> Option<Bytes> {
    if candidate.device.kind() == DeviceKind::Cpu {
        return policy.max_vram;
    }
    let available = inventory
        .devices
        .iter()
        .find(|device| device.id == candidate.device)
        .and_then(|device| device.memory_available.known().copied());
    minimum(policy.max_vram, available)
}

fn minimum<T: Ord + Copy>(left: Option<T>, right: Option<T>) -> Option<T> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn check_limit<T: Copy>(
    reasons: &mut Vec<PlannerReason>,
    label: &str,
    estimate: &Estimate<T>,
    limit: Option<T>,
    allow_unknown: bool,
    unknown_code: ReasonCode,
    exceeded_code: ReasonCode,
    raw: impl Fn(T) -> u64,
) {
    let Some(limit) = limit else { return };
    match estimate {
        Estimate::Unknown if !allow_unknown => reject(
            reasons,
            unknown_code,
            format!("{label} estimate is unknown under a hard limit"),
        ),
        Estimate::Known(value) if raw(*value) > raw(limit) => reject(
            reasons,
            exceeded_code,
            format!(
                "estimated {label} {} exceeds hard limit {}",
                raw(*value),
                raw(limit)
            ),
        ),
        Estimate::Known(_) | Estimate::Unknown => {}
    }
}

fn placement_matches(policy: &PlacementPolicy, candidate: &EngineCandidate) -> bool {
    match policy {
        PlacementPolicy::Auto | PlacementPolicy::Prefer(_) => true,
        PlacementPolicy::CpuOnly => candidate.device.kind() == DeviceKind::Cpu,
        PlacementPolicy::Require(device) => &candidate.device == device,
    }
}

fn reject(reasons: &mut Vec<PlannerReason>, code: ReasonCode, explanation: String) {
    reasons.push(PlannerReason { code, explanation });
}

fn candidate_identity(input: &CandidateInput) -> (String, String, String) {
    (
        input.candidate.engine_id.clone(),
        input.candidate.backend.to_string(),
        input.candidate.device.to_string(),
    )
}

fn compare_preferred_device(
    placement: &PlacementPolicy,
    left: &CandidateInput,
    right: &CandidateInput,
) -> Ordering {
    let PlacementPolicy::Prefer(preferred) = placement else {
        return Ordering::Equal;
    };
    let left_matches = left.candidate.device == *preferred;
    let right_matches = right.candidate.device == *preferred;
    right_matches.cmp(&left_matches)
}

fn compare_profile(profile: Profile, left: &CandidateInput, right: &CandidateInput) -> Ordering {
    let left_resources = &left.candidate.resources;
    let right_resources = &right.candidate.resources;
    let ordering = match profile {
        Profile::Fast => compare_known_low(
            &left_resources.latency,
            &right_resources.latency,
            Millis::get,
        ),
        Profile::Accurate => {
            compare_quality_high(&left_resources.quality, &right_resources.quality)
        }
        Profile::Cheap => compare_known_low(
            &left_resources.remote_cost,
            &right_resources.remote_cost,
            MicroUsd::get,
        ),
        Profile::Balanced
        | Profile::Cpu
        | Profile::LowVram
        | Profile::Offline
        | Profile::Private => {
            compare_quality_high(&left_resources.quality, &right_resources.quality)
                .then_with(|| {
                    compare_known_low(
                        &left_resources.latency,
                        &right_resources.latency,
                        Millis::get,
                    )
                })
                .then_with(|| {
                    compare_known_low(
                        &left_resources.remote_cost,
                        &right_resources.remote_cost,
                        MicroUsd::get,
                    )
                })
        }
    };
    ordering.then_with(|| candidate_identity(left).cmp(&candidate_identity(right)))
}

fn compare_known_low<T: Copy>(
    left: &Estimate<T>,
    right: &Estimate<T>,
    raw: impl Fn(T) -> u64,
) -> Ordering {
    match (left, right) {
        (Estimate::Known(left), Estimate::Known(right)) => raw(*left).cmp(&raw(*right)),
        (Estimate::Known(_), Estimate::Unknown) => Ordering::Less,
        (Estimate::Unknown, Estimate::Known(_)) => Ordering::Greater,
        (Estimate::Unknown, Estimate::Unknown) => Ordering::Equal,
    }
}

fn compare_quality_high(
    left: &Estimate<ferrodoc_core::Probability>,
    right: &Estimate<ferrodoc_core::Probability>,
) -> Ordering {
    match (left, right) {
        (Estimate::Known(left), Estimate::Known(right)) => right.get().total_cmp(&left.get()),
        (Estimate::Known(_), Estimate::Unknown) => Ordering::Less,
        (Estimate::Unknown, Estimate::Known(_)) => Ordering::Greater,
        (Estimate::Unknown, Estimate::Unknown) => Ordering::Equal,
    }
}

#[cfg(test)]
mod tests {
    use ferrodoc_core::{
        BackendId, EstimateConfidence, EstimateSource, Probability, ResourceEstimate,
    };
    use ferrodoc_engine_api::{EngineCompatibility, HardwareInventory};

    use super::*;

    fn inventory() -> HardwareInventory {
        HardwareInventory {
            logical_cpus: Estimate::Known(4),
            physical_cpus: Estimate::Known(2),
            cpu_source: Estimate::Known(EstimateSource {
                confidence: EstimateConfidence::Measured,
                method: "fixture".into(),
            }),
            ram_total: Estimate::Known(Bytes::new(8 * Bytes::GIB)),
            ram_available: Estimate::Known(Bytes::new(4 * Bytes::GIB)),
            ram_source: Estimate::Known(EstimateSource {
                confidence: EstimateConfidence::Measured,
                method: "fixture".into(),
            }),
            devices: Vec::new(),
        }
    }

    fn candidate(id: &str, ram: Estimate<Bytes>, vram: Estimate<Bytes>) -> CandidateInput {
        let backend = BackendId::new("fixture").unwrap();
        let device = ferrodoc_core::DeviceId::new(DeviceKind::Cpu, None).unwrap();
        CandidateInput {
            descriptor: EngineDescriptor {
                id: id.into(),
                version: "1.0.0".into(),
                capabilities: BTreeSet::from([Capability::OcrPage]),
                compatibility: vec![EngineCompatibility {
                    backend: backend.clone(),
                    devices: BTreeSet::from([DeviceKind::Cpu]),
                }],
                deterministic: true,
                network_use: NetworkUse::None,
                max_concurrency: 1,
            },
            candidate: EngineCandidate {
                engine_id: id.into(),
                backend,
                device,
                resources: ResourceEstimate {
                    peak_ram: ram,
                    warm_ram: Estimate::Known(Bytes::new(0)),
                    peak_vram: vram,
                    warm_vram: Estimate::Known(Bytes::new(0)),
                    latency: Estimate::Known(Millis::new(10)),
                    remote_cost: Estimate::Known(MicroUsd::new(0)),
                    quality: Estimate::Known(Probability::new(0.8).unwrap()),
                    source: Estimate::Unknown,
                },
            },
            model: ModelAvailability::NotRequired,
        }
    }

    #[test]
    fn unknown_hard_estimate_rejects_without_opt_in() {
        let mut policy = PlannerPolicy::for_profile(Profile::Balanced, Capability::OcrPage);
        policy.max_ram = Some(Bytes::new(Bytes::GIB));
        let report = plan(
            policy.clone(),
            &inventory(),
            [candidate(
                "unknown",
                Estimate::Unknown,
                Estimate::Known(Bytes::new(0)),
            )],
        );
        assert!(report.selected.is_none());
        assert_eq!(report.decisions[0].reasons[0].code, ReasonCode::RamUnknown);
        policy.allow_unknown_hard_estimates = true;
        assert!(
            plan(
                policy,
                &inventory(),
                [candidate(
                    "guarded",
                    Estimate::Unknown,
                    Estimate::Known(Bytes::new(0))
                )]
            )
            .selected
            .is_some()
        );
    }

    #[test]
    fn low_vram_is_a_hard_bound_and_never_fabricates_fallback() {
        let policy = PlannerPolicy::for_profile(Profile::LowVram, Capability::OcrPage);
        let report = plan(
            policy,
            &inventory(),
            [candidate(
                "large",
                Estimate::Known(Bytes::new(Bytes::GIB)),
                Estimate::Known(Bytes::new(3 * Bytes::GIB)),
            )],
        );
        assert!(report.selected.is_none());
        assert!(
            report.decisions[0]
                .reasons
                .iter()
                .any(|reason| reason.code == ReasonCode::VramBudgetExceeded)
        );
    }

    #[test]
    fn missing_model_and_private_network_use_have_distinct_codes() {
        let mut input = candidate(
            "remote",
            Estimate::Known(Bytes::new(Bytes::GIB)),
            Estimate::Known(Bytes::new(0)),
        );
        input.descriptor.network_use = NetworkUse::Required;
        input.model = ModelAvailability::Missing {
            id: ModelId::derive(&[b"missing"]),
        };
        let report = plan(
            PlannerPolicy::for_profile(Profile::Private, Capability::OcrPage),
            &inventory(),
            [input],
        );
        let codes: BTreeSet<_> = report.decisions[0]
            .reasons
            .iter()
            .map(|reason| reason.code)
            .collect();
        assert!(codes.contains(&ReasonCode::ModelUnavailable));
        assert!(codes.contains(&ReasonCode::PrivacyViolation));
    }

    #[test]
    fn fast_profile_selects_lowest_known_latency_stably() {
        let mut slow = candidate(
            "slow",
            Estimate::Known(Bytes::new(Bytes::GIB)),
            Estimate::Known(Bytes::new(0)),
        );
        slow.candidate.resources.latency = Estimate::Known(Millis::new(20));
        let fast = candidate(
            "fast",
            Estimate::Known(Bytes::new(Bytes::GIB)),
            Estimate::Known(Bytes::new(0)),
        );
        let report = plan(
            PlannerPolicy::for_profile(Profile::Fast, Capability::OcrPage),
            &inventory(),
            [slow, fast],
        );
        assert_eq!(report.selected.unwrap().engine_id, "fast");
        assert_eq!(report.decisions.len(), 2);
    }
}
