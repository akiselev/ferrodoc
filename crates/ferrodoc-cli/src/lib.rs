//! Stable v0.2 CLI JSON and exit-status contracts.

use ferrodoc_engine_api::HardwareInventory;
use ferrodoc_runtime::{ConversionPlan, planner::PlanningReport};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const CLI_ERROR_VERSION: &str = "ferrodoc-cli-error/1";
pub const EXIT_SUCCESS: u8 = 0;
pub const EXIT_ERROR: u8 = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CliErrorBody {
    pub category: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CliErrorEnvelope {
    pub schema_version: String,
    pub error: CliErrorBody,
}

impl CliErrorEnvelope {
    pub fn new(category: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            schema_version: CLI_ERROR_VERSION.into(),
            error: CliErrorBody {
                category: category.into(),
                message: message.into(),
            },
        }
    }
}

/// JSON contract emitted by `ferrodoc plan`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlanOutput {
    #[serde(flatten)]
    pub pipeline: ConversionPlan,
    pub inventory: HardwareInventory,
    pub resource_plans: Vec<PlanningReport>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_in_cli_schemas_match_contracts() {
        let error: serde_json::Value =
            serde_json::from_str(include_str!("../../../schemas/cli-error-v1.json")).unwrap();
        let plan: serde_json::Value =
            serde_json::from_str(include_str!("../../../schemas/cli-plan-v1.json")).unwrap();
        assert_eq!(
            error,
            serde_json::to_value(schemars::schema_for!(CliErrorEnvelope)).unwrap()
        );
        assert_eq!(
            plan,
            serde_json::to_value(schemars::schema_for!(PlanOutput)).unwrap()
        );
    }
}
