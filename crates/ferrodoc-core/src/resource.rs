//! Explicit resource estimates.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Bytes, MicroUsd, Millis, Probability};

/// A value that is either known or explicitly unknown.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", content = "value", rename_all = "snake_case")]
pub enum Estimate<T> {
    /// A measured or estimated value is available.
    Known(T),
    /// No defensible value is available.
    #[default]
    Unknown,
}

impl<T> Estimate<T> {
    /// Returns a reference to a known value.
    pub const fn known(&self) -> Option<&T> {
        match self {
            Self::Known(value) => Some(value),
            Self::Unknown => None,
        }
    }

    /// Returns true when no value is known.
    pub const fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown)
    }
}

/// Confidence class for an estimate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum EstimateConfidence {
    /// Directly measured for the same artifact and configuration.
    Measured,
    /// Derived from a calibrated model or close observation.
    Calibrated,
    /// Conservative static upper bound supplied by an engine.
    Conservative,
    /// Weak heuristic unsuitable for strict admission by default.
    Heuristic,
}

/// Provenance for a group of resource estimates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EstimateSource {
    /// Confidence class.
    pub confidence: EstimateConfidence,
    /// Stable method identifier or short description.
    pub method: String,
}

/// Resource estimates for one engine candidate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ResourceEstimate {
    /// Maximum host RAM expected while executing.
    pub peak_ram: Estimate<Bytes>,
    /// Host RAM retained while the engine remains warm.
    pub warm_ram: Estimate<Bytes>,
    /// Maximum device VRAM expected while executing.
    pub peak_vram: Estimate<Bytes>,
    /// Device VRAM retained while the engine remains warm.
    pub warm_vram: Estimate<Bytes>,
    /// Expected execution duration.
    pub latency: Estimate<Millis>,
    /// Expected remote monetary cost.
    pub remote_cost: Estimate<MicroUsd>,
    /// Expected output quality when calibrated.
    pub quality: Estimate<Probability>,
    /// Source and confidence for these values.
    pub source: Estimate<EstimateSource>,
}

impl Default for ResourceEstimate {
    fn default() -> Self {
        Self {
            peak_ram: Estimate::Unknown,
            warm_ram: Estimate::Unknown,
            peak_vram: Estimate::Unknown,
            warm_vram: Estimate::Unknown,
            latency: Estimate::Unknown,
            remote_cost: Estimate::Unknown,
            quality: Estimate::Unknown,
            source: Estimate::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_serialize_as_unknown_not_zero() {
        let estimate = ResourceEstimate::default();
        let json = serde_json::to_string(&estimate).unwrap();
        assert!(json.contains("\"status\":\"unknown\""));
        assert!(!json.contains("peak_ram\":0"));
        assert!(estimate.peak_ram.is_unknown());
    }
}
