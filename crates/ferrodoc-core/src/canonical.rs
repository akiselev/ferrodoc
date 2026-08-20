//! Canonical textual identifiers used by schemas, manifests, and protocol messages.

use std::{fmt, str::FromStr};

use schemars::{JsonSchema, r#gen::SchemaGenerator, schema::Schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

use crate::{CoreError, error::invalid_text};

macro_rules! string_schema {
    ($type:ty, $name:literal) => {
        impl Serialize for $type {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(&self.to_string())
            }
        }

        impl<'de> Deserialize<'de> for $type {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let input = String::deserialize(deserializer)?;
                input.parse().map_err(D::Error::custom)
            }
        }

        impl JsonSchema for $type {
            fn schema_name() -> String {
                $name.into()
            }

            fn json_schema(generator: &mut SchemaGenerator) -> Schema {
                String::json_schema(generator)
            }
        }
    };
}

/// A capability an engine can provide.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub enum Capability {
    /// Open and inspect a document container.
    #[serde(rename = "document.open")]
    #[schemars(rename = "document.open")]
    DocumentOpen,
    /// Render a document page.
    #[serde(rename = "page.render")]
    #[schemars(rename = "page.render")]
    PageRender,
    /// Extract native text.
    #[serde(rename = "text.extract")]
    #[schemars(rename = "text.extract")]
    TextExtract,
    /// Detect layout regions.
    #[serde(rename = "layout.detect")]
    #[schemars(rename = "layout.detect")]
    LayoutDetect,
    /// Determine reading order.
    #[serde(rename = "reading-order.detect")]
    #[schemars(rename = "reading-order.detect")]
    ReadingOrderDetect,
    /// OCR a complete page.
    #[serde(rename = "ocr.page")]
    #[schemars(rename = "ocr.page")]
    OcrPage,
    /// OCR one region.
    #[serde(rename = "ocr.region")]
    #[schemars(rename = "ocr.region")]
    OcrRegion,
    /// Recognize a table.
    #[serde(rename = "table.recognize")]
    #[schemars(rename = "table.recognize")]
    TableRecognize,
    /// Recognize a formula.
    #[serde(rename = "formula.recognize")]
    #[schemars(rename = "formula.recognize")]
    FormulaRecognize,
    /// Score evidence quality.
    #[serde(rename = "quality.score")]
    #[schemars(rename = "quality.score")]
    QualityScore,
}

impl Capability {
    /// Parses user-facing aliases at a CLI boundary. Serialization remains canonical.
    pub fn parse_cli(input: &str) -> Result<Self, CoreError> {
        match input {
            "ocr" => Ok(Self::OcrPage),
            "table" => Ok(Self::TableRecognize),
            "formula" => Ok(Self::FormulaRecognize),
            "quality" => Ok(Self::QualityScore),
            other => other.parse(),
        }
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::DocumentOpen => "document.open",
            Self::PageRender => "page.render",
            Self::TextExtract => "text.extract",
            Self::LayoutDetect => "layout.detect",
            Self::ReadingOrderDetect => "reading-order.detect",
            Self::OcrPage => "ocr.page",
            Self::OcrRegion => "ocr.region",
            Self::TableRecognize => "table.recognize",
            Self::FormulaRecognize => "formula.recognize",
            Self::QualityScore => "quality.score",
        })
    }
}

impl FromStr for Capability {
    type Err = CoreError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "document.open" => Ok(Self::DocumentOpen),
            "page.render" => Ok(Self::PageRender),
            "text.extract" => Ok(Self::TextExtract),
            "layout.detect" => Ok(Self::LayoutDetect),
            "reading-order.detect" => Ok(Self::ReadingOrderDetect),
            "ocr.page" => Ok(Self::OcrPage),
            "ocr.region" => Ok(Self::OcrRegion),
            "table.recognize" => Ok(Self::TableRecognize),
            "formula.recognize" => Ok(Self::FormulaRecognize),
            "quality.score" => Ok(Self::QualityScore),
            _ => Err(invalid_text(
                "capability",
                input,
                "expected a canonical dotted capability",
            )),
        }
    }
}

/// A built-in planning profile name.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
#[schemars(rename_all = "kebab-case")]
pub enum Profile {
    /// Prefer minimum latency.
    Fast,
    /// Balance quality and resource use.
    Balanced,
    /// Prefer quality.
    Accurate,
    /// Restrict work to CPU devices.
    Cpu,
    /// Enforce a small explicit VRAM budget.
    LowVram,
    /// Reject network-dependent candidates.
    Offline,
    /// Reject candidates that disclose document content.
    Private,
    /// Prefer minimum monetary cost.
    Cheap,
}

impl fmt::Display for Profile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Fast => "fast",
            Self::Balanced => "balanced",
            Self::Accurate => "accurate",
            Self::Cpu => "cpu",
            Self::LowVram => "low-vram",
            Self::Offline => "offline",
            Self::Private => "private",
            Self::Cheap => "cheap",
        })
    }
}

impl FromStr for Profile {
    type Err = CoreError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "fast" => Ok(Self::Fast),
            "balanced" => Ok(Self::Balanced),
            "accurate" => Ok(Self::Accurate),
            "cpu" => Ok(Self::Cpu),
            "low-vram" => Ok(Self::LowVram),
            "offline" => Ok(Self::Offline),
            "private" => Ok(Self::Private),
            "cheap" => Ok(Self::Cheap),
            _ => Err(invalid_text(
                "profile",
                input,
                "expected a canonical profile name",
            )),
        }
    }
}

/// A physical device family, separate from inference backend and placement policy.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum DeviceKind {
    /// Host CPU.
    Cpu,
    /// NVIDIA CUDA device.
    Cuda,
    /// Vulkan compute device.
    Vulkan,
    /// Apple Metal device.
    Metal,
    /// WebGPU-compatible device.
    Wgpu,
}

/// A canonical physical device identifier such as `cpu` or `cuda:0`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceId {
    kind: DeviceKind,
    index: Option<u32>,
}

impl DeviceId {
    /// Creates a device while enforcing which families require an index.
    pub fn new(kind: DeviceKind, index: Option<u32>) -> Result<Self, CoreError> {
        let valid = match kind {
            DeviceKind::Cpu | DeviceKind::Metal => index.is_none(),
            DeviceKind::Cuda | DeviceKind::Vulkan | DeviceKind::Wgpu => index.is_some(),
        };
        if !valid {
            return Err(invalid_text(
                "device",
                format!("{kind:?}:{index:?}"),
                "CPU and Metal are unindexed; CUDA, Vulkan, and WGPU require an index",
            ));
        }
        Ok(Self { kind, index })
    }

    /// Returns the physical device family.
    pub const fn kind(&self) -> DeviceKind {
        self.kind
    }

    /// Returns the device index when the family is indexed.
    pub const fn index(&self) -> Option<u32> {
        self.index
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.kind, self.index) {
            (DeviceKind::Cpu, None) => formatter.write_str("cpu"),
            (DeviceKind::Metal, None) => formatter.write_str("metal"),
            (DeviceKind::Cuda, Some(index)) => write!(formatter, "cuda:{index}"),
            (DeviceKind::Vulkan, Some(index)) => write!(formatter, "vulkan:{index}"),
            (DeviceKind::Wgpu, Some(index)) => write!(formatter, "wgpu:{index}"),
            _ => Err(fmt::Error),
        }
    }
}

impl FromStr for DeviceId {
    type Err = CoreError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "cpu" => return Self::new(DeviceKind::Cpu, None),
            "metal" => return Self::new(DeviceKind::Metal, None),
            _ => {}
        }
        let (kind, index) = input.split_once(':').ok_or_else(|| {
            invalid_text("device", input, "expected cpu, metal, or a kind:index pair")
        })?;
        let kind = match kind {
            "cuda" => DeviceKind::Cuda,
            "vulkan" => DeviceKind::Vulkan,
            "wgpu" => DeviceKind::Wgpu,
            _ => return Err(invalid_text("device", input, "unknown device family")),
        };
        let index = index
            .parse::<u32>()
            .map_err(|_| invalid_text("device", input, "invalid device index"))?;
        Self::new(kind, Some(index))
    }
}

string_schema!(DeviceId, "DeviceId");

/// A validated inference backend identifier, independent of physical device.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BackendId(String);

impl BackendId {
    /// Creates a lowercase backend identifier.
    pub fn new(input: impl Into<String>) -> Result<Self, CoreError> {
        let input = input.into();
        if valid_token(&input) {
            Ok(Self(input))
        } else {
            Err(invalid_text(
                "backend identifier",
                input,
                "expected lowercase ASCII letters, digits, '.', '_', or '-'",
            ))
        }
    }

    /// Returns the canonical identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BackendId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for BackendId {
    type Err = CoreError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Self::new(input)
    }
}

string_schema!(BackendId, "BackendId");

/// A normalized lowercase media type without parameters.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MediaType(String);

impl MediaType {
    /// Parses and normalizes an Internet media type.
    pub fn new(input: impl Into<String>) -> Result<Self, CoreError> {
        let input = input.into();
        let normalized = input.to_ascii_lowercase();
        let (top, sub) = normalized
            .split_once('/')
            .ok_or_else(|| invalid_text("media type", &input, "expected type/subtype"))?;
        if valid_token(top) && valid_token(sub) && !normalized.contains(';') {
            Ok(Self(normalized))
        } else {
            Err(invalid_text(
                "media type",
                input,
                "expected an ASCII type/subtype without parameters",
            ))
        }
    }

    /// Returns the canonical lowercase representation.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MediaType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for MediaType {
    type Err = CoreError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Self::new(input)
    }
}

string_schema!(MediaType, "MediaType");

/// Planner placement intent, distinct from backend compatibility and devices.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "policy", content = "device", rename_all = "kebab-case")]
pub enum PlacementPolicy {
    /// Let the planner select a compatible placement.
    Auto,
    /// Restrict execution to CPU.
    CpuOnly,
    /// Require one physical device.
    Require(DeviceId),
    /// Prefer one device but permit another compatible placement.
    Prefer(DeviceId),
}

fn valid_token(input: &str) -> bool {
    !input.is_empty()
        && input.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'.' | b'_' | b'-' | b'+')
        })
        && input.as_bytes()[0].is_ascii_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_representations_are_identical() {
        for capability in [
            Capability::DocumentOpen,
            Capability::PageRender,
            Capability::TextExtract,
            Capability::LayoutDetect,
            Capability::ReadingOrderDetect,
            Capability::OcrPage,
            Capability::OcrRegion,
            Capability::TableRecognize,
            Capability::FormulaRecognize,
            Capability::QualityScore,
        ] {
            let text = capability.to_string();
            assert_eq!(text.parse::<Capability>().unwrap(), capability);
            assert_eq!(
                serde_json::to_string(&capability).unwrap(),
                format!("\"{text}\"")
            );
        }
        assert!("ocr".parse::<Capability>().is_err());
        assert_eq!(Capability::parse_cli("ocr").unwrap(), Capability::OcrPage);
    }

    #[test]
    fn devices_do_not_mix_remote_or_hybrid_placement() {
        for text in ["cpu", "metal", "cuda:0", "vulkan:3", "wgpu:1"] {
            let device: DeviceId = text.parse().unwrap();
            assert_eq!(device.to_string(), text);
            assert_eq!(
                serde_json::from_str::<DeviceId>(&format!("\"{text}\"")).unwrap(),
                device
            );
        }
        assert!("remote".parse::<DeviceId>().is_err());
        assert!("hybrid".parse::<DeviceId>().is_err());
        assert!(DeviceId::new(DeviceKind::Cpu, Some(0)).is_err());
    }

    #[test]
    fn media_types_normalize_once() {
        let media = MediaType::new("Application/PDF").unwrap();
        assert_eq!(media.as_str(), "application/pdf");
        assert_eq!(
            serde_json::to_string(&media).unwrap(),
            "\"application/pdf\""
        );
        assert!(MediaType::new("application/pdf; charset=utf-8").is_err());
    }
}
