//! Checked integer and fixed-point quantities.

use std::{fmt, str::FromStr};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{CoreError, error::invalid_number, error::invalid_text};

/// A byte count. SI and IEC suffixes retain their distinct multipliers when parsed.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(transparent)]
pub struct Bytes(u64);

impl Bytes {
    /// One SI kilobyte.
    pub const KB: u64 = 1_000;
    /// One SI megabyte.
    pub const MB: u64 = 1_000_000;
    /// One SI gigabyte.
    pub const GB: u64 = 1_000_000_000;
    /// One IEC kibibyte.
    pub const KIB: u64 = 1_024;
    /// One IEC mebibyte.
    pub const MIB: u64 = 1_048_576;
    /// One IEC gibibyte.
    pub const GIB: u64 = 1_073_741_824;

    /// Creates a byte count.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the raw byte count.
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Checked addition.
    pub fn checked_add(self, other: Self) -> Result<Self, CoreError> {
        self.0
            .checked_add(other.0)
            .map(Self)
            .ok_or(CoreError::ArithmeticOverflow { kind: "bytes" })
    }

    /// Checked subtraction.
    pub fn checked_sub(self, other: Self) -> Result<Self, CoreError> {
        self.0
            .checked_sub(other.0)
            .map(Self)
            .ok_or(CoreError::ArithmeticOverflow { kind: "bytes" })
    }

    /// Checked multiplication by an integer.
    pub fn checked_mul(self, factor: u64) -> Result<Self, CoreError> {
        self.0
            .checked_mul(factor)
            .map(Self)
            .ok_or(CoreError::ArithmeticOverflow { kind: "bytes" })
    }
}

impl fmt::Display for Bytes {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (unit, multiplier) in [("GiB", Self::GIB), ("MiB", Self::MIB), ("KiB", Self::KIB)] {
            if self.0 >= multiplier && self.0.is_multiple_of(multiplier) {
                return write!(formatter, "{} {unit}", self.0 / multiplier);
            }
        }
        write!(formatter, "{} B", self.0)
    }
}

impl FromStr for Bytes {
    type Err = CoreError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let (number, suffix) = split_number_suffix(input, "bytes")?;
        let multiplier = match suffix {
            "" | "B" => 1,
            "KB" => Self::KB,
            "MB" => Self::MB,
            "GB" => Self::GB,
            "KiB" => Self::KIB,
            "MiB" => Self::MIB,
            "GiB" => Self::GIB,
            _ => {
                return Err(invalid_text(
                    "bytes",
                    input,
                    "expected B, KB, MB, GB, KiB, MiB, or GiB",
                ));
            }
        };
        parse_scaled_u64(number, multiplier, "bytes").map(Self)
    }
}

/// A monetary amount in millionths of a US dollar.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(transparent)]
pub struct MicroUsd(u64);

impl MicroUsd {
    /// Creates an amount from integer microdollars.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns integer microdollars.
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Checked addition.
    pub fn checked_add(self, other: Self) -> Result<Self, CoreError> {
        self.0
            .checked_add(other.0)
            .map(Self)
            .ok_or(CoreError::ArithmeticOverflow { kind: "micro-USD" })
    }
}

impl fmt::Display for MicroUsd {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} microUSD", self.0)
    }
}

/// A duration in whole milliseconds.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(transparent)]
pub struct Millis(u64);

impl Millis {
    /// Creates a millisecond duration.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the millisecond count.
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Checked addition.
    pub fn checked_add(self, other: Self) -> Result<Self, CoreError> {
        self.0
            .checked_add(other.0)
            .map(Self)
            .ok_or(CoreError::ArithmeticOverflow {
                kind: "milliseconds",
            })
    }
}

impl From<std::time::Duration> for Millis {
    fn from(value: std::time::Duration) -> Self {
        Self(value.as_millis().min(u128::from(u64::MAX)) as u64)
    }
}

/// A finite probability in the inclusive range zero through one.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "f64", into = "f64")]
pub struct Probability(f64);

impl Probability {
    /// Creates a checked probability.
    pub fn new(value: f64) -> Result<Self, CoreError> {
        if value.is_finite() && (0.0..=1.0).contains(&value) {
            Ok(Self(value))
        } else {
            Err(invalid_number(
                "probability",
                "expected a finite value between zero and one",
            ))
        }
    }

    /// Returns the numeric probability.
    pub const fn get(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for Probability {
    type Error = CoreError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Probability> for f64 {
    fn from(value: Probability) -> Self {
        value.0
    }
}

fn split_number_suffix<'a>(
    input: &'a str,
    kind: &'static str,
) -> Result<(&'a str, &'a str), CoreError> {
    let trimmed = input.trim();
    if trimmed.starts_with('-') || trimmed.starts_with('+') {
        return Err(invalid_text(kind, input, "signs are not accepted"));
    }
    let split = trimmed
        .find(|character: char| !character.is_ascii_digit() && character != '.')
        .unwrap_or(trimmed.len());
    let (number, suffix) = trimmed.split_at(split);
    if number.is_empty() {
        return Err(invalid_text(kind, input, "missing numeric value"));
    }
    Ok((number, suffix.trim()))
}

fn parse_scaled_u64(number: &str, multiplier: u64, kind: &'static str) -> Result<u64, CoreError> {
    if number.matches('.').count() > 1 {
        return Err(invalid_text(kind, number, "multiple decimal points"));
    }
    let (whole, fraction) = number.split_once('.').unwrap_or((number, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(invalid_text(kind, number, "invalid decimal number"));
    }
    if fraction.len() > 9 {
        return Err(invalid_text(kind, number, "excessive decimal precision"));
    }

    let whole = whole
        .parse::<u128>()
        .map_err(|_| invalid_text(kind, number, "integer overflow"))?;
    let mut value = whole
        .checked_mul(u128::from(multiplier))
        .ok_or(CoreError::ArithmeticOverflow { kind })?;
    if !fraction.is_empty() {
        let numerator = fraction
            .parse::<u128>()
            .map_err(|_| invalid_text(kind, number, "invalid fraction"))?;
        let denominator = 10_u128.pow(fraction.len() as u32);
        let scaled = numerator
            .checked_mul(u128::from(multiplier))
            .ok_or(CoreError::ArithmeticOverflow { kind })?;
        if !scaled.is_multiple_of(denominator) {
            return Err(invalid_text(
                kind,
                number,
                "fraction cannot be represented exactly",
            ));
        }
        value = value
            .checked_add(scaled / denominator)
            .ok_or(CoreError::ArithmeticOverflow { kind })?;
    }
    u64::try_from(value).map_err(|_| CoreError::ArithmeticOverflow { kind })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinguishes_si_and_iec_units() {
        assert_eq!("1 MB".parse::<Bytes>().unwrap().get(), 1_000_000);
        assert_eq!("1 MiB".parse::<Bytes>().unwrap().get(), 1_048_576);
        assert_eq!("1.5 KiB".parse::<Bytes>().unwrap().get(), 1_536);
    }

    #[test]
    fn rejects_invalid_and_overflowing_quantities() {
        for input in ["NaN", "inf", "-1 MiB", "0.1 B", "1.0000000001 GiB"] {
            assert!(input.parse::<Bytes>().is_err(), "accepted {input}");
        }
        assert!(format!("{} GiB", u64::MAX).parse::<Bytes>().is_err());
        assert!(Bytes::new(u64::MAX).checked_add(Bytes::new(1)).is_err());
        assert!(Bytes::new(0).checked_sub(Bytes::new(1)).is_err());
    }

    #[test]
    fn byte_display_round_trips() {
        for value in [0, 1, 1_000, Bytes::KIB, Bytes::MIB * 3, u64::MAX] {
            let bytes = Bytes::new(value);
            assert_eq!(bytes.to_string().parse::<Bytes>().unwrap(), bytes);
        }
    }

    #[test]
    fn probability_deserialization_validates() {
        assert!(serde_json::from_str::<Probability>("0.5").is_ok());
        assert!(serde_json::from_str::<Probability>("1.1").is_err());
    }
}
