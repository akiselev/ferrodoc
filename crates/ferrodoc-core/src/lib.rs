//! Stable, runtime-agnostic types shared by the Ferrodoc workspace.

use std::{
    collections::BTreeMap,
    fmt,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    str::FromStr,
};

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use uuid::Uuid;

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid digest: {0}")]
    InvalidDigest(String),
    #[error("invalid resource quantity: {0}")]
    InvalidQuantity(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn area(self) -> f32 {
        self.width.max(0.0) * self.height.max(0.0)
    }

    pub fn right(self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(self) -> f32 {
        self.y + self.height
    }

    pub fn intersects(self, other: Self) -> bool {
        self.x < other.right()
            && self.right() > other.x
            && self.y < other.bottom()
            && self.bottom() > other.y
    }

    pub fn intersection(self, other: Self) -> Option<Self> {
        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = self.right().min(other.right());
        let y2 = self.bottom().min(other.bottom());
        (x2 > x1 && y2 > y1).then(|| Self::new(x1, y1, x2 - x1, y2 - y1))
    }

    pub fn iou(self, other: Self) -> f32 {
        let intersection = self.intersection(other).map_or(0.0, Self::area);
        if intersection == 0.0 {
            return 0.0;
        }
        intersection / (self.area() + other.area() - intersection)
    }

    pub fn expand(self, margin: f32, page: Option<Self>) -> Self {
        let mut out = Self::new(
            self.x - margin,
            self.y - margin,
            self.width + 2.0 * margin,
            self.height + 2.0 * margin,
        );
        if let Some(page) = page {
            out.x = out.x.max(page.x);
            out.y = out.y.max(page.y);
            out.width = out.right().min(page.right()) - out.x;
            out.height = out.bottom().min(page.bottom()) - out.y;
        }
        out
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum RegionKind {
    Unknown,
    Page,
    Paragraph,
    Heading,
    List,
    ListItem,
    Table,
    TableCell,
    Equation,
    Figure,
    Caption,
    Code,
    Form,
    Header,
    Footer,
    Marginalia,
    Footnote,
    Stamp,
    Handwriting,
}

impl fmt::Display for RegionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            serde_json::to_value(self)
                .unwrap_or_default()
                .as_str()
                .unwrap_or("unknown")
        )
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    DocumentOpen,
    PageRender,
    TextExtract,
    LayoutDetect,
    ReadingOrderDetect,
    OcrPage,
    OcrRegion,
    TableRecognize,
    FormulaRecognize,
    ChartRecognize,
    HandwritingRecognize,
    QualityScore,
    DocumentRefine,
    RoutePredict,
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::DocumentOpen => "document.open",
            Self::PageRender => "page.render",
            Self::TextExtract => "text.extract",
            Self::LayoutDetect => "layout.detect",
            Self::ReadingOrderDetect => "reading_order.detect",
            Self::OcrPage => "ocr.page",
            Self::OcrRegion => "ocr.region",
            Self::TableRecognize => "table.recognize",
            Self::FormulaRecognize => "formula.recognize",
            Self::ChartRecognize => "chart.recognize",
            Self::HandwritingRecognize => "handwriting.recognize",
            Self::QualityScore => "quality.score",
            Self::DocumentRefine => "document.refine",
            Self::RoutePredict => "route.predict",
        };
        f.write_str(s)
    }
}

impl FromStr for Capability {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "document.open" => Self::DocumentOpen,
            "page.render" => Self::PageRender,
            "text.extract" => Self::TextExtract,
            "layout.detect" => Self::LayoutDetect,
            "reading_order.detect" => Self::ReadingOrderDetect,
            "ocr.page" | "ocr" => Self::OcrPage,
            "ocr.region" => Self::OcrRegion,
            "table.recognize" | "table" => Self::TableRecognize,
            "formula.recognize" | "formula" => Self::FormulaRecognize,
            "chart.recognize" | "chart" => Self::ChartRecognize,
            "handwriting.recognize" | "handwriting" => Self::HandwritingRecognize,
            "quality.score" | "quality" => Self::QualityScore,
            "document.refine" | "refine" => Self::DocumentRefine,
            "route.predict" | "router" => Self::RoutePredict,
            _ => return Err(format!("unknown capability {s:?}")),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EngineClass {
    Deterministic,
    ClassicalOcr,
    NeuralOcr,
    Layout,
    VisionLanguageModel,
    NativeRustModel,
    Onnx,
    Remote,
    Router,
    Utility,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Device {
    Cpu,
    Cuda { index: u32 },
    Vulkan { index: u32 },
    Metal,
    Wgpu,
    Remote,
    Hybrid,
}

impl Device {
    pub fn is_gpu(&self) -> bool {
        matches!(
            self,
            Self::Cuda { .. } | Self::Vulkan { .. } | Self::Metal | Self::Wgpu | Self::Hybrid
        )
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    Default,
)]
pub struct Bytes(pub u64);

impl Bytes {
    pub const KIB: u64 = 1024;
    pub const MIB: u64 = 1024 * Self::KIB;
    pub const GIB: u64 = 1024 * Self::MIB;
    pub const fn mib(value: u64) -> Self {
        Self(value * Self::MIB)
    }
    pub const fn gib(value: u64) -> Self {
        Self(value * Self::GIB)
    }
    pub fn saturating_sub(self, rhs: Self) -> Self {
        Self(self.0.saturating_sub(rhs.0))
    }
}

impl fmt::Display for Bytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 >= Self::GIB {
            write!(f, "{:.2} GiB", self.0 as f64 / Self::GIB as f64)
        } else if self.0 >= Self::MIB {
            write!(f, "{:.1} MiB", self.0 as f64 / Self::MIB as f64)
        } else if self.0 >= Self::KIB {
            write!(f, "{:.1} KiB", self.0 as f64 / Self::KIB as f64)
        } else {
            write!(f, "{} B", self.0)
        }
    }
}

impl FromStr for Bytes {
    type Err = CoreError;
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let s = input.trim().to_ascii_lowercase();
        let split = s
            .find(|c: char| !c.is_ascii_digit() && c != '.')
            .unwrap_or(s.len());
        let (number, suffix) = s.split_at(split);
        let n: f64 = number
            .parse()
            .map_err(|_| CoreError::InvalidQuantity(input.into()))?;
        let multiplier = match suffix.trim() {
            "" | "b" => 1.0,
            "k" | "kb" | "kib" => Self::KIB as f64,
            "m" | "mb" | "mib" => Self::MIB as f64,
            "g" | "gb" | "gib" => Self::GIB as f64,
            other => return Err(CoreError::InvalidQuantity(other.into())),
        };
        Ok(Self((n * multiplier) as u64))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct Digest {
    pub algorithm: String,
    pub hex: String,
}

impl Digest {
    pub fn sha256_bytes(bytes: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        Self {
            algorithm: "sha256".into(),
            hex: hex::encode(hasher.finalize()),
        }
    }

    pub fn sha256_file(path: impl AsRef<Path>) -> Result<Self, CoreError> {
        let mut file = File::open(path)?;
        let mut hasher = Sha256::new();
        let mut buf = [0u8; 1024 * 128];
        loop {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Ok(Self {
            algorithm: "sha256".into(),
            hex: hex::encode(hasher.finalize()),
        })
    }

    pub fn as_key(&self) -> String {
        format!("{}-{}", self.algorithm, self.hex)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BlobRef {
    pub path: PathBuf,
    #[serde(default)]
    pub offset: u64,
    #[serde(default)]
    pub len: Option<u64>,
    pub media_type: String,
    pub digest: Option<Digest>,
}

impl BlobRef {
    pub fn file(path: impl Into<PathBuf>, media_type: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            offset: 0,
            len: None,
            media_type: media_type.into(),
            digest: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ResourceEstimate {
    #[serde(default)]
    pub peak_ram: Bytes,
    #[serde(default)]
    pub peak_vram: Bytes,
    #[serde(default)]
    pub warm_ram: Bytes,
    #[serde(default)]
    pub warm_vram: Bytes,
    #[serde(default)]
    pub latency_ms: u64,
    #[serde(default)]
    pub visual_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub remote_cost_microusd: u64,
    #[serde(default)]
    pub quality_hint: f32,
    #[serde(default)]
    pub notes: Vec<String>,
}

impl Default for ResourceEstimate {
    fn default() -> Self {
        Self {
            peak_ram: Bytes::default(),
            peak_vram: Bytes::default(),
            warm_ram: Bytes::default(),
            warm_vram: Bytes::default(),
            latency_ms: 0,
            visual_tokens: 0,
            output_tokens: 0,
            remote_cost_microusd: 0,
            quality_hint: 0.5,
            notes: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct HardwareInventory {
    pub logical_cpus: usize,
    pub ram_total: Bytes,
    pub ram_available: Bytes,
    #[serde(default)]
    pub gpus: Vec<GpuInventory>,
    pub os: String,
    pub arch: String,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GpuInventory {
    pub index: u32,
    pub name: String,
    pub backend: String,
    pub memory_total: Option<Bytes>,
    pub memory_available: Option<Bytes>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

impl HardwareInventory {
    pub fn conservative_local() -> Self {
        Self {
            logical_cpus: std::thread::available_parallelism()
                .map(|x| x.get())
                .unwrap_or(1),
            ram_total: Bytes::default(),
            ram_available: Bytes::default(),
            gpus: vec![],
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            metadata: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct ModelRef {
    pub id: String,
    pub revision: Option<String>,
    #[serde(default)]
    pub files: Vec<PathBuf>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Provenance {
    pub source: String,
    pub engine: Option<String>,
    pub engine_version: Option<String>,
    pub model: Option<String>,
    pub model_revision: Option<String>,
    pub input_digest: Option<Digest>,
    #[serde(default)]
    pub parameters: BTreeMap<String, serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

impl Provenance {
    pub fn native(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            engine: None,
            engine_version: None,
            model: None,
            model_revision: None,
            input_digest: None,
            parameters: BTreeMap::new(),
            created_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct RunId(pub Uuid);
impl RunId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}
impl Default for RunId {
    fn default() -> Self {
        Self::new()
    }
}
impl fmt::Display for RunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_parse() {
        assert_eq!("1.5GiB".parse::<Bytes>().unwrap().0, 1610612736);
        assert_eq!("512m".parse::<Bytes>().unwrap(), Bytes::mib(512));
    }

    #[test]
    fn rect_iou() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(5.0, 5.0, 10.0, 10.0);
        assert!((a.iou(b) - 25.0 / 175.0).abs() < 1e-5);
    }
}
