//! PDF acquisition contract and parser limits.
//!
//! Phase 1 intentionally contains no PDF parser. Phase 2 supplies inspection,
//! native extraction, and deterministic rasterization behind this boundary.

use ferrodoc_core::{Bytes, ScopedBlob, Sha256Digest};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Limits applied before an untrusted PDF parser receives input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PdfLimits {
    /// Maximum input byte count.
    pub maximum_input_bytes: Bytes,
    /// Maximum accepted page count.
    pub maximum_pages: u32,
    /// Maximum decoded objects.
    pub maximum_objects: u64,
    /// Maximum recursive object depth.
    pub maximum_depth: u32,
    /// Maximum raster pixels for one page.
    pub maximum_page_pixels: u64,
}

impl Default for PdfLimits {
    fn default() -> Self {
        Self {
            maximum_input_bytes: Bytes::new(256 * Bytes::MIB),
            maximum_pages: 10_000,
            maximum_objects: 2_000_000,
            maximum_depth: 128,
            maximum_page_pixels: 200_000_000,
        }
    }
}

/// Acquired immutable PDF identity passed to the Phase 2 inspector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PdfInput {
    /// Host-scoped input bytes.
    pub blob: ScopedBlob,
    /// Verified whole-input digest.
    pub digest: Sha256Digest,
}

impl PdfInput {
    /// Applies input-size policy before parsing.
    pub fn validate(&self, limits: &PdfLimits) -> Result<(), PdfError> {
        if self.blob.range.len() > limits.maximum_input_bytes.get() {
            Err(PdfError::LimitExceeded {
                limit: "maximum_input_bytes",
                actual: self.blob.range.len(),
                maximum: limits.maximum_input_bytes.get(),
            })
        } else {
            Ok(())
        }
    }
}

/// Stable PDF-boundary error categories.
#[derive(Debug, Error)]
pub enum PdfError {
    /// A configured parser limit was exceeded.
    #[error("PDF {limit} exceeded: actual {actual}, maximum {maximum}")]
    LimitExceeded {
        /// Limit name.
        limit: &'static str,
        /// Observed value.
        actual: u64,
        /// Configured maximum.
        maximum: u64,
    },
    /// Parser functionality is deliberately absent in Phase 1.
    #[error("PDF inspection is not implemented in the Phase 1 skeleton")]
    NotImplemented,
}

#[cfg(test)]
mod tests {
    use ferrodoc_core::{BlobId, BlobRange, MediaType};

    use super::*;

    #[test]
    fn rejects_oversized_input_before_parsing() {
        let input = PdfInput {
            blob: ScopedBlob {
                id: BlobId::new("pdf-1").unwrap(),
                range: BlobRange::new(0, 11).unwrap(),
                media_type: MediaType::new("application/pdf").unwrap(),
                expected_digest: None,
            },
            digest: Sha256Digest::of_bytes(b"not parsed"),
        };
        let limits = PdfLimits {
            maximum_input_bytes: Bytes::new(10),
            ..PdfLimits::default()
        };
        assert!(matches!(
            input.validate(&limits),
            Err(PdfError::LimitExceeded { .. })
        ));
    }
}
