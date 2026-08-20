//! Core validation errors.

use thiserror::Error;

/// An invariant violation in a runtime-agnostic Ferrodoc value.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CoreError {
    /// A textual value is malformed or non-canonical.
    #[error("invalid {kind}: {value:?} ({reason})")]
    InvalidText {
        /// The logical value kind.
        kind: &'static str,
        /// The rejected input.
        value: String,
        /// A stable explanation suitable for diagnostics.
        reason: &'static str,
    },
    /// A numeric value is non-finite, negative, overflowing, or otherwise invalid.
    #[error("invalid {kind}: {reason}")]
    InvalidNumber {
        /// The logical value kind.
        kind: &'static str,
        /// A stable explanation suitable for diagnostics.
        reason: &'static str,
    },
    /// Values belong to incompatible coordinate systems or units.
    #[error("incompatible geometry: {0}")]
    IncompatibleGeometry(&'static str),
    /// Checked arithmetic overflowed or underflowed.
    #[error("{kind} arithmetic overflow")]
    ArithmeticOverflow {
        /// The quantity kind.
        kind: &'static str,
    },
    /// File hashing failed.
    #[error("I/O error: {0}")]
    Io(String),
}

impl From<std::io::Error> for CoreError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.to_string())
    }
}

pub(crate) fn invalid_text(
    kind: &'static str,
    value: impl Into<String>,
    reason: &'static str,
) -> CoreError {
    CoreError::InvalidText {
        kind,
        value: value.into(),
        reason,
    }
}

pub(crate) const fn invalid_number(kind: &'static str, reason: &'static str) -> CoreError {
    CoreError::InvalidNumber { kind, reason }
}
