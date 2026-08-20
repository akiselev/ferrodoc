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
