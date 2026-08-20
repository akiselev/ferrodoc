//! Versioned evidence graph for Ferrodoc documents.
//!
//! Evidence is append-only: native extraction, OCR, layout, and refinement remain
//! separate records. A selected view references evidence IDs and explains the
//! reconciliation decision without deleting competing hypotheses.

use std::collections::{BTreeMap, BTreeSet};

use ferrodoc_core::{
    ArtifactId, BlobId, CURRENT_SCHEMA_VERSION, DeterministicProvenance, DocumentId, EvidenceId,
    LayerId, MediaType, PageId, PageRect, Probability, RegionId, SchemaVersion, Sha256Digest,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Validation or canonical serialization error for an evidence graph.
#[derive(Debug, Error)]
pub enum IrError {
    /// A graph invariant was violated.
    #[error("invalid evidence graph: {0}")]
    Invalid(String),
    /// Canonical JSON serialization failed.
    #[error("serialize evidence graph: {0}")]
    Serialize(#[from] serde_json::Error),
}

/// Document metadata with deterministic extension ordering.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DocumentMetadata {
    /// Optional title.
    pub title: Option<String>,
    /// Ordered author names.
    #[serde(default)]
    pub authors: Vec<String>,
    /// Optional BCP 47 language tag supplied by the source.
    pub language: Option<String>,
    /// Additional deterministic metadata.
    #[serde(default)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// Provenance-bearing source layer kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "name", rename_all = "snake_case")]
pub enum SourceLayerKind {
    /// Native PDF content.
    NativePdf,
    /// Rasterized source content.
    Raster,
    /// OCR hypothesis layer.
    Ocr,
    /// Layout hypothesis layer.
    Layout,
    /// Forward-compatible producer-defined kind.
    Unknown(String),
}

/// One immutable evidence-producing layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SourceLayer {
    /// Stable layer identity.
    pub id: LayerId,
    /// Layer kind.
    pub kind: SourceLayerKind,
    /// Deterministic producing provenance.
    pub provenance: DeterministicProvenance,
}

/// A deterministic raster or other derived artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RenderArtifact {
    /// Stable artifact identity.
    pub id: ArtifactId,
    /// Host-scoped blob token. No host path is serialized.
    pub blob_id: BlobId,
    /// Content digest.
    pub digest: Sha256Digest,
    /// Media type.
    pub media_type: MediaType,
    /// Raster width when applicable.
    pub width: Option<u32>,
    /// Raster height when applicable.
    pub height: Option<u32>,
}

/// Semantic region kind with one canonical snake-case representation.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum RegionKind {
    /// Unknown or unclassified content.
    Unknown,
    /// Paragraph text.
    Paragraph,
    /// Heading text.
    Heading,
    /// List container.
    List,
    /// List item.
    ListItem,
    /// Table.
    Table,
    /// Table cell.
    TableCell,
    /// Mathematical formula.
    Formula,
    /// Figure or illustration.
    Figure,
    /// Caption.
    Caption,
    /// Source code.
    Code,
    /// Repeating page header.
    Header,
    /// Repeating page footer.
    Footer,
    /// Footnote.
    Footnote,
    /// Handwriting.
    Handwriting,
}

/// A structured table cell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TableCell {
    /// Zero-based row.
    pub row: u32,
    /// Zero-based column.
    pub column: u32,
    /// Number of rows occupied.
    pub row_span: u32,
    /// Number of columns occupied.
    pub column_span: u32,
    /// Cell text.
    pub text: String,
}

/// Typed evidence payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EvidenceContent {
    /// Plain or styled text represented as Unicode.
    Text {
        /// Extracted text.
        text: String,
    },
    /// Structured table cells.
    Table {
        /// Row count.
        rows: u32,
        /// Column count.
        columns: u32,
        /// Cells with explicit spans.
        cells: Vec<TableCell>,
    },
    /// Mathematical formula.
    Formula {
        /// Normalized LaTeX source.
        latex: String,
    },
    /// Image reference.
    Image {
        /// Stable render artifact identity.
        artifact: ArtifactId,
        /// Optional alternative text.
        alt: Option<String>,
    },
    /// Forward-compatible payload retained without interpretation.
    Unknown {
        /// Producer-defined media type.
        media_type: MediaType,
        /// JSON payload.
        value: serde_json::Value,
    },
}

/// One hypothesis produced by a source layer or engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Evidence {
    /// Stable evidence identity.
    pub id: EvidenceId,
    /// Source layer that produced this evidence.
    pub layer_id: LayerId,
    /// Typed content.
    pub content: EvidenceContent,
    /// Optional page geometry.
    pub geometry: Option<PageRect>,
    /// Optional calibrated confidence.
    pub confidence: Option<Probability>,
    /// Deterministic provenance.
    pub provenance: DeterministicProvenance,
    /// Engine-provided deterministic metadata.
    #[serde(default)]
    pub engine_metadata: BTreeMap<String, serde_json::Value>,
}

/// Machine-readable selection reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum SelectionReason {
    /// Native evidence passed the configured quality policy.
    NativeQuality,
    /// OCR was selected because native evidence was absent.
    NativeAbsent,
    /// OCR was selected because native evidence was below threshold.
    NativeBelowThreshold,
    /// Multiple records were deterministically reconciled.
    Reconciled,
    /// A caller explicitly selected evidence.
    UserOverride,
}

/// Selected view over one or more evidence records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SelectedView {
    /// Evidence records consumed by the selection.
    pub evidence_ids: Vec<EvidenceId>,
    /// Machine-readable reason.
    pub reason: SelectionReason,
    /// Human-readable deterministic explanation.
    pub explanation: String,
}

/// A semantic page region containing competing evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Region {
    /// Stable region identity.
    pub id: RegionId,
    /// Semantic kind.
    pub kind: RegionKind,
    /// Page geometry.
    pub geometry: PageRect,
    /// Append-only evidence records.
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    /// Optional selected view.
    pub selected: Option<SelectedView>,
}

/// Directed reading-order edge.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
pub struct ReadingOrderEdge {
    /// Region read first.
    pub before: RegionId,
    /// Region read next.
    pub after: RegionId,
}

/// One document page and all evidence attached to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Page {
    /// Stable page identity.
    pub id: PageId,
    /// Zero-based page index.
    pub index: u32,
    /// Complete page geometry.
    pub bounds: PageRect,
    /// Source layers in deterministic order.
    #[serde(default)]
    pub layers: Vec<SourceLayer>,
    /// Render artifacts in deterministic order.
    #[serde(default)]
    pub artifacts: Vec<RenderArtifact>,
    /// Semantic regions in deterministic order.
    #[serde(default)]
    pub regions: Vec<Region>,
    /// Reading-order graph edges.
    #[serde(default)]
    pub reading_order: Vec<ReadingOrderEdge>,
}

/// A versioned evidence graph for a document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Document {
    /// Persistent schema version.
    pub schema_version: SchemaVersion,
    /// Stable document identity.
    pub id: DocumentId,
    /// Original input digest.
    pub input_digest: Sha256Digest,
    /// Source metadata.
    pub metadata: DocumentMetadata,
    /// Ordered pages.
    pub pages: Vec<Page>,
}

impl Document {
    /// Validates cross-reference, uniqueness, geometry, page-index, and DAG invariants.
    pub fn validate(&self) -> Result<(), IrError> {
        if self.schema_version.major != CURRENT_SCHEMA_VERSION.major {
            return Err(IrError::Invalid(
                "unsupported IR schema major version".into(),
            ));
        }
        let mut page_ids = BTreeSet::new();
        let mut page_indexes = BTreeSet::new();
        for page in &self.pages {
            if !page_ids.insert(&page.id) || !page_indexes.insert(page.index) {
                return Err(IrError::Invalid("duplicate page ID or index".into()));
            }
            if page.bounds.page_index != page.index {
                return Err(IrError::Invalid(
                    "page bounds use a different page index".into(),
                ));
            }
            validate_page(page)?;
        }
        Ok(())
    }

    /// Serializes validated IR as compact deterministic JSON.
    pub fn to_canonical_json(&self) -> Result<Vec<u8>, IrError> {
        self.validate()?;
        Ok(serde_json::to_vec(self)?)
    }
}

fn validate_page(page: &Page) -> Result<(), IrError> {
    let layer_ids: BTreeSet<_> = page.layers.iter().map(|layer| &layer.id).collect();
    if layer_ids.len() != page.layers.len() {
        return Err(IrError::Invalid("duplicate source layer ID".into()));
    }
    let artifact_ids: BTreeSet<_> = page.artifacts.iter().map(|artifact| &artifact.id).collect();
    if artifact_ids.len() != page.artifacts.len() {
        return Err(IrError::Invalid("duplicate render artifact ID".into()));
    }
    let region_ids: BTreeSet<_> = page.regions.iter().map(|region| &region.id).collect();
    if region_ids.len() != page.regions.len() {
        return Err(IrError::Invalid("duplicate region ID".into()));
    }
    let mut evidence_ids = BTreeSet::new();
    for region in &page.regions {
        if region.geometry.page_index != page.index {
            return Err(IrError::Invalid(
                "region geometry uses a different page index".into(),
            ));
        }
        if !page
            .bounds
            .rect
            .contains(region.geometry.rect)
            .map_err(|error| IrError::Invalid(error.to_string()))?
        {
            return Err(IrError::Invalid("region lies outside page bounds".into()));
        }
        for evidence in &region.evidence {
            if !evidence_ids.insert(&evidence.id) {
                return Err(IrError::Invalid("duplicate evidence ID".into()));
            }
            if !layer_ids.contains(&evidence.layer_id) {
                return Err(IrError::Invalid(
                    "evidence references an unknown layer".into(),
                ));
            }
            if let Some(geometry) = evidence.geometry
                && geometry.page_index != page.index
            {
                return Err(IrError::Invalid(
                    "evidence geometry uses a different page index".into(),
                ));
            }
            if let EvidenceContent::Image { artifact, .. } = &evidence.content
                && !artifact_ids.contains(artifact)
            {
                return Err(IrError::Invalid(
                    "image evidence references an unknown artifact".into(),
                ));
            }
        }
        if let Some(selected) = &region.selected
            && (selected.evidence_ids.is_empty()
                || selected
                    .evidence_ids
                    .iter()
                    .any(|id| !region.evidence.iter().any(|evidence| &evidence.id == id)))
        {
            return Err(IrError::Invalid(
                "selected view references missing evidence".into(),
            ));
        }
    }
    for edge in &page.reading_order {
        if edge.before == edge.after
            || !region_ids.contains(&edge.before)
            || !region_ids.contains(&edge.after)
        {
            return Err(IrError::Invalid("invalid reading-order edge".into()));
        }
    }
    ensure_acyclic(&region_ids, &page.reading_order)
}

fn ensure_acyclic(
    region_ids: &BTreeSet<&RegionId>,
    edges: &[ReadingOrderEdge],
) -> Result<(), IrError> {
    let mut indegree: BTreeMap<&RegionId, usize> =
        region_ids.iter().copied().map(|id| (id, 0)).collect();
    let mut outgoing: BTreeMap<&RegionId, Vec<&RegionId>> = BTreeMap::new();
    for edge in edges {
        *indegree.get_mut(&edge.after).expect("validated edge") += 1;
        outgoing.entry(&edge.before).or_default().push(&edge.after);
    }
    let mut ready: Vec<_> = indegree
        .iter()
        .filter_map(|(id, degree)| (*degree == 0).then_some(*id))
        .collect();
    let mut visited = 0;
    while let Some(id) = ready.pop() {
        visited += 1;
        if let Some(next) = outgoing.get(id) {
            for target in next {
                let degree = indegree.get_mut(target).expect("validated edge");
                *degree -= 1;
                if *degree == 0 {
                    ready.push(target);
                }
            }
        }
    }
    if visited == region_ids.len() {
        Ok(())
    } else {
        Err(IrError::Invalid(
            "reading-order graph contains a cycle".into(),
        ))
    }
}

#[cfg(test)]
mod tests;
