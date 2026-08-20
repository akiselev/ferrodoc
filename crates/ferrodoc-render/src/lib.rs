//! Deterministic rendering boundary.
//!
//! Phase 1 exposes full evidence JSON only. Markdown and semantic HTML enter with
//! the Phase 2 vertical slice.

use ferrodoc_ir::{Document, IrError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Stable output format name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum OutputFormat {
    /// Full evidence graph as canonical JSON.
    EvidenceJson,
    /// Planned for Phase 2.
    Markdown,
    /// Planned for Phase 2.
    Html,
}

/// Rendering failure.
#[derive(Debug, Error)]
pub enum RenderError {
    /// IR validation or serialization failed.
    #[error(transparent)]
    Ir(#[from] IrError),
    /// The format is outside the current phase's implemented surface.
    #[error("output format {0:?} is not implemented yet")]
    Unsupported(OutputFormat),
}

/// Renders the one format implemented by the Phase 1 skeleton.
pub fn render(document: &Document, format: OutputFormat) -> Result<Vec<u8>, RenderError> {
    match format {
        OutputFormat::EvidenceJson => document.to_canonical_json().map_err(Into::into),
        OutputFormat::Markdown | OutputFormat::Html => Err(RenderError::Unsupported(format)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unimplemented_formats_fail_explicitly() {
        assert!(matches!(
            render(
                &serde_json::from_slice(include_bytes!("../../../fixtures/document-ir-v1.json"))
                    .unwrap(),
                OutputFormat::Markdown
            ),
            Err(RenderError::Unsupported(OutputFormat::Markdown))
        ));
    }
}
