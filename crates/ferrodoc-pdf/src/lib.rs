//! Bounded pure-Rust PDF inspection, native text extraction, and rasterization.

use ferrodoc_core::{
    Bytes, CoordinateSpace, CoordinateTransform, PageRect, Rect, ScopedBlob, Sha256Digest, Unit,
};
use hayro::{RenderCache, RenderSettings, hayro_interpret::InterpreterSettings, hayro_syntax::Pdf};
use lopdf::{Document as LoDocument, Object, ObjectId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Limits applied around untrusted PDF parsing and rendering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PdfLimits {
    /// Maximum input byte count.
    pub maximum_input_bytes: Bytes,
    /// Maximum accepted page count.
    pub maximum_pages: u32,
    /// Maximum loaded indirect objects.
    pub maximum_objects: u64,
    /// Maximum inherited page-tree walk depth.
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

/// Acquired immutable PDF identity.
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
        check_input_size(self.blob.range.len(), limits)
    }
}

/// Native text associated with page geometry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NativeTextSpan {
    /// Extracted Unicode text.
    pub text: String,
    /// Best geometry available from the parser. Phase 2 uses page bounds when glyph boxes are absent.
    pub geometry: PageRect,
}

/// Inspected PDF page metadata and native evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PdfPage {
    /// Zero-based page index.
    pub index: u32,
    /// Inherited media box in PDF points.
    pub media_box: Rect,
    /// Inherited crop box in PDF points.
    pub crop_box: Rect,
    /// Clockwise page rotation in degrees.
    pub rotation: i16,
    /// Native text spans. Empty means no defensible native text was recovered.
    #[serde(default)]
    pub native_text: Vec<NativeTextSpan>,
}

/// Immutable inspection result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PdfInspection {
    /// Input content digest.
    pub digest: Sha256Digest,
    /// Input byte count.
    pub bytes: Bytes,
    /// Ordered pages.
    pub pages: Vec<PdfPage>,
}

/// Cheap, deterministic classification of a PDF page before OCR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum PageContentHint {
    /// No native text, image invocation, or painted vector path was observed.
    Blank,
    /// Native text was observed without image invocations.
    BornDigital,
    /// Image invocations were observed without native text.
    Scanned,
    /// Native text and image invocations were both observed.
    Hybrid,
    /// Content operators were observed but native text and image evidence were inconclusive.
    OtherContent,
}

/// Per-page evidence gathered without rasterization or OCR.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PdfPageSurvey {
    /// Zero-based page index.
    pub page_index: u32,
    /// Effective crop-box width in PDF points.
    pub width_points: f64,
    /// Effective crop-box height in PDF points.
    pub height_points: f64,
    /// Number of non-whitespace native Unicode scalar values.
    pub native_characters: u64,
    /// Native character count divided by page area in square points.
    pub native_characters_per_square_point: f64,
    /// Number of PDF image/form invocation operators. Forms are not misreported as images.
    pub xobject_invocations: u64,
    /// XObject invocation count divided by page area in square points.
    pub xobject_invocations_per_square_point: f64,
    /// Number of PDF text-show operators, including text that could not be decoded.
    pub text_show_operations: u64,
    /// Number of painted vector-path operators.
    pub painted_vector_paths: u64,
    /// Painted vector-path count divided by page area in square points.
    pub painted_vector_paths_per_square_point: f64,
    /// Coarse pre-OCR content hint.
    pub content_hint: PageContentHint,
    /// Unicode script families observed in native text.
    pub script_hints: Vec<String>,
    /// Exact normalized native-text fingerprint, absent when no text was recovered.
    pub native_text_sha256: Option<Sha256Digest>,
    /// Deterministic 64-bit token SimHash for near-duplicate candidate generation.
    pub native_text_simhash: Option<String>,
    /// Whether refinement is likely valuable because native text is absent or mixed with imagery.
    pub high_value_candidate: bool,
    /// Conservative page-level candidate kinds for later region refinement.
    pub candidate_kinds: Vec<String>,
}

/// Exact repeated first/last native-line hint across pages; this is not a header/footer decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RepeatedMarginTextHint {
    /// `first_native_line` or `last_native_line`.
    pub position: String,
    /// Exact normalized line fingerprint; source text is not duplicated in the survey.
    pub text_sha256: Sha256Digest,
    /// Pages on which the exact line occurs in that position.
    pub page_indexes: Vec<u32>,
}

/// Cheap deterministic survey completed before any page rasterization or OCR.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PdfSurvey {
    /// Exact source identity.
    pub digest: Sha256Digest,
    /// Container byte count.
    pub bytes: Bytes,
    /// Loaded indirect-object count.
    pub object_count: u64,
    /// Selected deterministic PDF Info/Catalog metadata. Missing keys remain absent.
    pub container_metadata: std::collections::BTreeMap<String, String>,
    /// Ordered page observations.
    pub pages: Vec<PdfPageSurvey>,
    /// Document-level script families observed in native text.
    pub script_hints: Vec<String>,
    /// Coarse deterministic family features suitable for grouping, not a family decision.
    pub family_features: Vec<String>,
    /// Exact repeated edge-line candidates for later header/footer classification.
    pub repeated_margin_text_hints: Vec<RepeatedMarginTextHint>,
}

/// Deterministic RGBA8 page raster.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RasterPage {
    /// Zero-based page index.
    pub page_index: u32,
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// Row-major opaque RGBA8 bytes.
    pub rgba: Vec<u8>,
}

/// Parsed immutable PDF with reusable inspection.
pub struct PdfDocument {
    bytes: Vec<u8>,
    syntax: LoDocument,
    limits: PdfLimits,
    inspection: PdfInspection,
}

impl PdfDocument {
    /// Parses an in-memory PDF after applying the pre-parse byte limit.
    pub fn from_bytes(bytes: Vec<u8>, limits: PdfLimits) -> Result<Self, PdfError> {
        check_input_size(bytes.len() as u64, &limits)?;
        if !bytes.starts_with(b"%PDF-") {
            return Err(PdfError::Malformed("missing PDF header".into()));
        }
        let digest = Sha256Digest::of_bytes(&bytes);
        let syntax =
            LoDocument::load_mem(&bytes).map_err(|error| PdfError::Malformed(error.to_string()))?;
        if syntax.is_encrypted() {
            return Err(PdfError::Encrypted);
        }
        if syntax.objects.len() as u64 > limits.maximum_objects {
            return Err(PdfError::LimitExceeded {
                limit: "maximum_objects",
                actual: syntax.objects.len() as u64,
                maximum: limits.maximum_objects,
            });
        }
        let pages = inspect_pages(&syntax, &limits)?;
        let inspection = PdfInspection {
            digest,
            bytes: Bytes::new(bytes.len() as u64),
            pages,
        };
        Ok(Self {
            bytes,
            syntax,
            limits,
            inspection,
        })
    }

    /// Returns the deterministic inspection.
    pub const fn inspection(&self) -> &PdfInspection {
        &self.inspection
    }

    /// Surveys container and content-stream evidence without rendering or OCR.
    pub fn survey(&self) -> Result<PdfSurvey, PdfError> {
        let page_ids = self.syntax.get_pages();
        let mut pages = Vec::with_capacity(self.inspection.pages.len());
        let mut native_texts = Vec::with_capacity(self.inspection.pages.len());
        let mut all_scripts = std::collections::BTreeSet::new();
        for page in &self.inspection.pages {
            let page_id = *page_ids
                .values()
                .nth(page.index as usize)
                .ok_or(PdfError::PageOutOfRange(page.index))?;
            let content = self.syntax.get_page_content(page_id);
            let operations = lopdf::content::Content::decode(&content)
                .map_err(|error| PdfError::Malformed(error.to_string()))?
                .operations;
            let xobject_invocations = operations
                .iter()
                .filter(|operation| operation.operator == "Do")
                .count() as u64;
            let painted_vector_paths = operations
                .iter()
                .filter(|operation| {
                    matches!(
                        operation.operator.as_str(),
                        "S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*"
                    )
                })
                .count() as u64;
            let text_show_operations = operations
                .iter()
                .filter(|operation| matches!(operation.operator.as_str(), "Tj" | "TJ" | "'" | "\""))
                .count() as u64;
            let native_text = page
                .native_text
                .iter()
                .map(|span| span.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            native_texts.push(native_text.clone());
            let native_characters = native_text
                .chars()
                .filter(|character| !character.is_whitespace())
                .count() as u64;
            let scripts = script_hints(&native_text);
            all_scripts.extend(scripts.iter().cloned());
            let content_hint = match (native_characters > 0, xobject_invocations > 0) {
                (true, true) => PageContentHint::Hybrid,
                (true, false) => PageContentHint::BornDigital,
                (false, true) => PageContentHint::Scanned,
                (false, false) if painted_vector_paths > 0 || text_show_operations > 0 => {
                    PageContentHint::OtherContent
                }
                (false, false) => PageContentHint::Blank,
            };
            let area = page.crop_box.width() * page.crop_box.height();
            pages.push(PdfPageSurvey {
                page_index: page.index,
                width_points: page.crop_box.width(),
                height_points: page.crop_box.height(),
                native_characters,
                native_characters_per_square_point: if area > 0.0 {
                    native_characters as f64 / area
                } else {
                    0.0
                },
                xobject_invocations,
                xobject_invocations_per_square_point: if area > 0.0 {
                    xobject_invocations as f64 / area
                } else {
                    0.0
                },
                text_show_operations,
                painted_vector_paths,
                painted_vector_paths_per_square_point: if area > 0.0 {
                    painted_vector_paths as f64 / area
                } else {
                    0.0
                },
                content_hint,
                script_hints: scripts,
                native_text_sha256: (!native_text.is_empty())
                    .then(|| Sha256Digest::of_bytes(native_text.as_bytes())),
                native_text_simhash: token_simhash(&native_text),
                high_value_candidate: matches!(
                    content_hint,
                    PageContentHint::Scanned
                        | PageContentHint::Hybrid
                        | PageContentHint::OtherContent
                ),
                candidate_kinds: candidate_kinds(
                    &native_text,
                    xobject_invocations,
                    painted_vector_paths,
                ),
            });
        }
        let mut family_features = pages
            .iter()
            .map(|page| {
                format!(
                    "page-size:{:.0}x{:.0}",
                    page.width_points, page.height_points
                )
            })
            .collect::<std::collections::BTreeSet<_>>();
        family_features.insert(format!("pages:{}", pages.len()));
        for kind in pages.iter().map(|page| page.content_hint) {
            family_features.insert(format!("content:{}", content_hint_name(kind)));
        }
        Ok(PdfSurvey {
            digest: self.inspection.digest,
            bytes: self.inspection.bytes,
            object_count: self.syntax.objects.len() as u64,
            container_metadata: container_metadata(&self.syntax),
            pages,
            script_hints: all_scripts.into_iter().collect(),
            family_features: family_features.into_iter().collect(),
            repeated_margin_text_hints: repeated_margin_text_hints(&native_texts),
        })
    }

    /// Renders one page at the requested DPI using the pure-Rust Hayro renderer.
    pub fn render_page(&self, page_index: u32, dpi: u32) -> Result<RasterPage, PdfError> {
        if dpi == 0 || dpi > 1_200 {
            return Err(PdfError::Unsupported(
                "DPI must be between 1 and 1200".into(),
            ));
        }
        let metadata = self
            .inspection
            .pages
            .get(page_index as usize)
            .ok_or(PdfError::PageOutOfRange(page_index))?;
        let scale = f64::from(dpi) / 72.0;
        let (points_w, points_h) = if matches!(metadata.rotation, 90 | 270) {
            (metadata.crop_box.height(), metadata.crop_box.width())
        } else {
            (metadata.crop_box.width(), metadata.crop_box.height())
        };
        let width = scaled_dimension(points_w, scale)?;
        let height = scaled_dimension(points_h, scale)?;
        let pixels =
            u64::from(width)
                .checked_mul(u64::from(height))
                .ok_or(PdfError::LimitExceeded {
                    limit: "maximum_page_pixels",
                    actual: u64::MAX,
                    maximum: self.limits.maximum_page_pixels,
                })?;
        if pixels > self.limits.maximum_page_pixels {
            return Err(PdfError::LimitExceeded {
                limit: "maximum_page_pixels",
                actual: pixels,
                maximum: self.limits.maximum_page_pixels,
            });
        }
        let pdf = Pdf::new(self.bytes.clone())
            .map_err(|error| PdfError::Unsupported(format!("Hayro rejected PDF: {error:?}")))?;
        let page = pdf
            .pages()
            .get(page_index as usize)
            .ok_or(PdfError::PageOutOfRange(page_index))?;
        let cache = RenderCache::new();
        let pixmap = hayro::render(
            page,
            &cache,
            &InterpreterSettings::default(),
            &RenderSettings {
                x_scale: scale as f32,
                y_scale: scale as f32,
                width: Some(width as u16),
                height: Some(height as u16),
                bg_color: hayro::vello_cpu::color::palette::css::WHITE,
            },
        );
        Ok(RasterPage {
            page_index,
            width,
            height,
            rgba: pixmap.data_as_u8_slice().to_vec(),
        })
    }

    /// Returns the parser object count for trace and limit diagnostics.
    pub fn object_count(&self) -> usize {
        self.syntax.objects.len()
    }
}

const fn content_hint_name(hint: PageContentHint) -> &'static str {
    match hint {
        PageContentHint::Blank => "blank",
        PageContentHint::BornDigital => "born_digital",
        PageContentHint::Scanned => "scanned",
        PageContentHint::Hybrid => "hybrid",
        PageContentHint::OtherContent => "other_content",
    }
}

fn inspect_pages(document: &LoDocument, limits: &PdfLimits) -> Result<Vec<PdfPage>, PdfError> {
    let pages = document.get_pages();
    if pages.len() as u32 > limits.maximum_pages {
        return Err(PdfError::LimitExceeded {
            limit: "maximum_pages",
            actual: pages.len() as u64,
            maximum: u64::from(limits.maximum_pages),
        });
    }
    pages
        .into_iter()
        .enumerate()
        .map(|(index, (page_number, page_id))| {
            let media_box = inherited_box(document, page_id, b"MediaBox", limits.maximum_depth)?;
            let crop_box =
                inherited_box_optional(document, page_id, b"CropBox", limits.maximum_depth)?
                    .unwrap_or(media_box);
            let rotation = inherited_integer(document, page_id, b"Rotate", limits.maximum_depth)?
                .unwrap_or(0)
                .rem_euclid(360) as i16;
            if !matches!(rotation, 0 | 90 | 180 | 270) {
                return Err(PdfError::Unsupported(format!(
                    "page {page_number} has unsupported rotation {rotation}"
                )));
            }
            let text = document
                .extract_text(&[page_number])
                .map_err(|error| PdfError::Unsupported(format!("native text: {error}")))?;
            let text = normalize_native_text(&text);
            let native_text = if text.is_empty() {
                Vec::new()
            } else {
                vec![NativeTextSpan {
                    text,
                    geometry: PageRect {
                        page_index: index as u32,
                        rect: crop_box,
                        source_transform: CoordinateTransform::IDENTITY,
                    },
                }]
            };
            Ok(PdfPage {
                index: index as u32,
                media_box,
                crop_box,
                rotation,
                native_text,
            })
        })
        .collect()
}

fn inherited_box(
    document: &LoDocument,
    page_id: ObjectId,
    key: &[u8],
    maximum_depth: u32,
) -> Result<Rect, PdfError> {
    inherited_box_optional(document, page_id, key, maximum_depth)?.ok_or_else(|| {
        PdfError::Malformed(format!(
            "missing inherited {}",
            String::from_utf8_lossy(key)
        ))
    })
}

fn inherited_box_optional(
    document: &LoDocument,
    page_id: ObjectId,
    key: &[u8],
    maximum_depth: u32,
) -> Result<Option<Rect>, PdfError> {
    let Some(object) = inherited_object(document, page_id, key, maximum_depth)? else {
        return Ok(None);
    };
    let values = object
        .as_array()
        .map_err(|error| PdfError::Malformed(error.to_string()))?;
    if values.len() != 4 {
        return Err(PdfError::Malformed(
            "page box must contain four numbers".into(),
        ));
    }
    let number = |index: usize| {
        values[index]
            .as_float()
            .map(f64::from)
            .map_err(|error| PdfError::Malformed(error.to_string()))
    };
    let x1 = number(0)?;
    let y1 = number(1)?;
    let x2 = number(2)?;
    let y2 = number(3)?;
    if x2 < x1 || y2 < y1 {
        return Err(PdfError::Malformed("page box edges are reversed".into()));
    }
    Rect::new(x1, y1, x2 - x1, y2 - y1, CoordinateSpace::Pdf, Unit::Point)
        .map(Some)
        .map_err(|error| PdfError::Malformed(error.to_string()))
}

fn inherited_integer(
    document: &LoDocument,
    page_id: ObjectId,
    key: &[u8],
    maximum_depth: u32,
) -> Result<Option<i64>, PdfError> {
    inherited_object(document, page_id, key, maximum_depth)?
        .map(|object| {
            object
                .as_i64()
                .map_err(|error| PdfError::Malformed(error.to_string()))
        })
        .transpose()
}

fn inherited_object<'a>(
    document: &'a LoDocument,
    mut object_id: ObjectId,
    key: &[u8],
    maximum_depth: u32,
) -> Result<Option<&'a Object>, PdfError> {
    for _ in 0..maximum_depth {
        let dictionary = document
            .get_dictionary(object_id)
            .map_err(|error| PdfError::Malformed(error.to_string()))?;
        if let Ok(value) = dictionary.get_deref(key, document) {
            return Ok(Some(value));
        }
        object_id = match dictionary.get(b"Parent").and_then(Object::as_reference) {
            Ok(parent) => parent,
            Err(_) => return Ok(None),
        };
    }
    Err(PdfError::LimitExceeded {
        limit: "maximum_depth",
        actual: u64::from(maximum_depth) + 1,
        maximum: u64::from(maximum_depth),
    })
}

fn normalize_native_text(text: &str) -> String {
    text.replace('\0', "").trim().to_string()
}

fn container_metadata(document: &LoDocument) -> std::collections::BTreeMap<String, String> {
    let mut metadata = std::collections::BTreeMap::new();
    if let Ok(info) = document
        .trailer
        .get_deref(b"Info", document)
        .and_then(Object::as_dict)
    {
        for (pdf_key, stable_key) in [
            (b"Title".as_slice(), "title"),
            (b"Author".as_slice(), "author"),
            (b"Subject".as_slice(), "subject"),
            (b"Keywords".as_slice(), "keywords"),
            (b"Creator".as_slice(), "creator"),
            (b"Producer".as_slice(), "producer"),
        ] {
            if let Ok(bytes) = info.get(pdf_key).and_then(Object::as_str) {
                let value = String::from_utf8_lossy(bytes).trim().to_string();
                if !value.is_empty() {
                    metadata.insert(stable_key.into(), value);
                }
            }
        }
    }
    if let Ok(catalog) = document.catalog()
        && let Ok(bytes) = catalog.get(b"Lang").and_then(Object::as_str)
    {
        let value = String::from_utf8_lossy(bytes).trim().to_string();
        if !value.is_empty() {
            metadata.insert("language".into(), value);
        }
    }
    metadata
}

fn script_hints(text: &str) -> Vec<String> {
    let mut scripts = std::collections::BTreeSet::new();
    for character in text.chars().filter(|character| character.is_alphabetic()) {
        let code = character as u32;
        let script = if code <= 0x024f {
            "latin"
        } else if (0x0370..=0x03ff).contains(&code) {
            "greek"
        } else if (0x0400..=0x052f).contains(&code) {
            "cyrillic"
        } else if (0x0590..=0x05ff).contains(&code) {
            "hebrew"
        } else if (0x0600..=0x06ff).contains(&code) {
            "arabic"
        } else if (0x3040..=0x30ff).contains(&code) {
            "japanese_kana"
        } else if (0x3400..=0x9fff).contains(&code) {
            "han"
        } else if (0xac00..=0xd7af).contains(&code) {
            "hangul"
        } else {
            "other"
        };
        scripts.insert(script.to_string());
    }
    scripts.into_iter().collect()
}

fn candidate_kinds(text: &str, xobjects: u64, painted_paths: u64) -> Vec<String> {
    let mut kinds = std::collections::BTreeSet::new();
    if xobjects > 0 {
        kinds.insert("figure".to_string());
    }
    if text.lines().any(|line| line.contains(['|', '\t'])) {
        kinds.insert("table".to_string());
    }
    if text.contains(['=', '∑', '∫', '√']) {
        kinds.insert("formula".to_string());
    }
    if painted_paths > 0 {
        kinds.insert("vector_graphic".to_string());
    }
    kinds.into_iter().collect()
}

fn repeated_margin_text_hints(texts: &[String]) -> Vec<RepeatedMarginTextHint> {
    let mut occurrences: std::collections::BTreeMap<(&str, String), Vec<u32>> =
        std::collections::BTreeMap::new();
    for (page_index, text) in texts.iter().enumerate() {
        let lines: Vec<_> = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect();
        if let Some(first) = lines.first() {
            occurrences
                .entry(("first_native_line", (*first).to_string()))
                .or_default()
                .push(page_index as u32);
        }
        if let Some(last) = lines.last()
            && lines.first() != Some(last)
        {
            occurrences
                .entry(("last_native_line", (*last).to_string()))
                .or_default()
                .push(page_index as u32);
        }
    }
    occurrences
        .into_iter()
        .filter(|(_, pages)| pages.len() > 1)
        .map(|((position, text), page_indexes)| RepeatedMarginTextHint {
            position: position.into(),
            text_sha256: Sha256Digest::of_bytes(text.as_bytes()),
            page_indexes,
        })
        .collect()
}

fn token_simhash(text: &str) -> Option<String> {
    let tokens: Vec<_> = text
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_lowercase)
        .collect();
    if tokens.is_empty() {
        return None;
    }
    let mut weights = [0_i64; 64];
    for token in tokens {
        let digest = Sha256Digest::of_bytes(token.as_bytes());
        let mut first = [0_u8; 8];
        first.copy_from_slice(&digest.as_bytes()[..8]);
        let fingerprint = u64::from_be_bytes(first);
        for (bit, weight) in weights.iter_mut().enumerate() {
            *weight += if fingerprint & (1_u64 << bit) == 0 {
                -1
            } else {
                1
            };
        }
    }
    let fingerprint = weights
        .iter()
        .enumerate()
        .fold(0_u64, |value, (bit, weight)| {
            value | (u64::from(*weight >= 0) << bit)
        });
    Some(format!("{fingerprint:016x}"))
}

fn check_input_size(actual: u64, limits: &PdfLimits) -> Result<(), PdfError> {
    if actual > limits.maximum_input_bytes.get() {
        Err(PdfError::LimitExceeded {
            limit: "maximum_input_bytes",
            actual,
            maximum: limits.maximum_input_bytes.get(),
        })
    } else {
        Ok(())
    }
}

fn scaled_dimension(points: f64, scale: f64) -> Result<u32, PdfError> {
    let pixels = (points * scale).ceil();
    if !pixels.is_finite() || pixels < 1.0 || pixels > f64::from(u16::MAX) {
        return Err(PdfError::Unsupported(
            "render dimension is outside the supported u16 range".into(),
        ));
    }
    Ok(pixels as u32)
}

/// Stable PDF-boundary errors.
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
    /// The input is not a structurally valid PDF.
    #[error("malformed PDF: {0}")]
    Malformed(String),
    /// Encrypted input requires credentials and is not accepted by the offline path.
    #[error("encrypted PDF is not supported")]
    Encrypted,
    /// The valid PDF uses an unsupported feature or render size.
    #[error("unsupported PDF: {0}")]
    Unsupported(String),
    /// Requested page does not exist.
    #[error("PDF page index {0} is out of range")]
    PageOutOfRange(u32),
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

    #[test]
    fn malformed_input_is_categorized() {
        assert!(matches!(
            PdfDocument::from_bytes(b"not a pdf".to_vec(), PdfLimits::default()),
            Err(PdfError::Malformed(_))
        ));
    }

    #[test]
    fn fixture_modes_preserve_native_evidence_distinction() {
        let born_digital = PdfDocument::from_bytes(
            include_bytes!("../../../fixtures/pdf/born-digital.pdf").to_vec(),
            PdfLimits::default(),
        )
        .unwrap();
        assert!(
            born_digital.inspection().pages[0].native_text[0]
                .text
                .contains("FERRODOC FIXTURE HEADING")
        );

        let image_only = PdfDocument::from_bytes(
            include_bytes!("../../../fixtures/pdf/image-only.pdf").to_vec(),
            PdfLimits::default(),
        )
        .unwrap();
        assert!(image_only.inspection().pages[0].native_text.is_empty());
        let raster = image_only.render_page(0, 96).unwrap();
        assert_eq!(
            raster.rgba.len(),
            (raster.width * raster.height * 4) as usize
        );

        let hybrid = PdfDocument::from_bytes(
            include_bytes!("../../../fixtures/pdf/hybrid.pdf").to_vec(),
            PdfLimits::default(),
        )
        .unwrap();
        assert_eq!(
            hybrid.inspection().pages[0].native_text[0].text,
            "HYBRID NATIVE HEADING"
        );
    }

    #[test]
    fn cheap_survey_distinguishes_mixed_fixture_modes_without_rendering() {
        let survey = |bytes: &[u8]| {
            PdfDocument::from_bytes(bytes.to_vec(), PdfLimits::default())
                .unwrap()
                .survey()
                .unwrap()
        };
        let born = survey(include_bytes!("../../../fixtures/pdf/born-digital.pdf"));
        let scan = survey(include_bytes!("../../../fixtures/pdf/image-only.pdf"));
        let hybrid = survey(include_bytes!("../../../fixtures/pdf/hybrid.pdf"));
        assert_eq!(born.pages[0].content_hint, PageContentHint::BornDigital);
        assert_eq!(scan.pages[0].content_hint, PageContentHint::Scanned);
        assert_eq!(hybrid.pages[0].content_hint, PageContentHint::Hybrid);
        assert!(born.pages[0].native_text_sha256.is_some());
        assert!(born.pages[0].native_text_simhash.is_some());
        assert!(born.pages[0].script_hints.contains(&"latin".into()));
        assert_eq!(scan.pages[0].native_text_sha256, None);
        assert!(scan.pages[0].high_value_candidate);
        assert_eq!(
            born,
            survey(include_bytes!("../../../fixtures/pdf/born-digital.pdf"))
        );
    }

    #[test]
    fn checked_in_survey_schema_and_golden_match_contract() {
        let document = PdfDocument::from_bytes(
            include_bytes!("../../../fixtures/pdf/born-digital.pdf").to_vec(),
            PdfLimits::default(),
        )
        .unwrap();
        let expected: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/pdf-survey-born-digital-v1.json"
        ))
        .unwrap();
        assert_eq!(
            expected,
            serde_json::to_value(document.survey().unwrap()).unwrap()
        );
        let expected_schema: serde_json::Value =
            serde_json::from_str(include_str!("../../../schemas/pdf-survey-v1.json")).unwrap();
        assert_eq!(
            expected_schema,
            serde_json::to_value(schemars::schema_for!(PdfSurvey)).unwrap()
        );
    }

    #[test]
    fn rotated_crop_box_controls_geometry_and_raster_limits() {
        let limits = PdfLimits {
            maximum_page_pixels: 10,
            ..PdfLimits::default()
        };
        let document = PdfDocument::from_bytes(
            include_bytes!("../../../fixtures/pdf/rotated-cropped.pdf").to_vec(),
            limits,
        )
        .unwrap();
        let page = &document.inspection().pages[0];
        assert_eq!(page.rotation, 90);
        assert_eq!(page.crop_box.width(), 400.0);
        assert_eq!(page.crop_box.height(), 600.0);
        assert!(matches!(
            document.render_page(0, 72),
            Err(PdfError::LimitExceeded {
                limit: "maximum_page_pixels",
                ..
            })
        ));
    }

    #[test]
    fn checked_in_malformed_fixture_is_rejected() {
        assert!(matches!(
            PdfDocument::from_bytes(
                include_bytes!("../../../fixtures/pdf/malformed.pdf").to_vec(),
                PdfLimits::default()
            ),
            Err(PdfError::Malformed(_))
        ));
    }

    #[test]
    fn checked_in_encrypted_fixture_is_categorized() {
        assert!(matches!(
            PdfDocument::from_bytes(
                include_bytes!("../../../fixtures/pdf/encrypted.pdf").to_vec(),
                PdfLimits::default()
            ),
            Err(PdfError::Encrypted)
        ));
    }

    #[test]
    fn unsupported_rotation_is_categorized() {
        assert!(matches!(
            PdfDocument::from_bytes(
                include_bytes!("../../../fixtures/pdf/unsupported-rotation.pdf").to_vec(),
                PdfLimits::default()
            ),
            Err(PdfError::Unsupported(_))
        ));
    }
}
