//! Deterministic, versioned PDF corpus generation with explicit truth provenance.

#[macro_use]
extern crate lopdf;

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use ferrodoc_core::{CURRENT_SCHEMA_VERSION, SchemaVersion, Sha256Digest};
use ferrodoc_pdf::{PdfDocument, PdfLimits};
use lopdf::{
    Document, Object, Stream,
    content::{Content, Operation},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// Version of generator semantics and output identity.
pub const GENERATOR_VERSION: &str = "ferrodoc-foundry/1";
const PAGE_WIDTH: i64 = 595;
const PAGE_HEIGHT: i64 = 842;

/// Redistribution status for an asset used by generated documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum RedistributionStatus {
    /// Asset bytes may be redistributed under the recorded license.
    Redistributable,
    /// Asset is a PDF/system built-in and contributes no bundled bytes.
    BuiltIn,
    /// Asset may only be referenced locally and cannot enter a published corpus.
    LocalOnly,
}

/// One explicitly declared font or other generator asset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AssetSpec {
    /// Stable asset identifier.
    pub id: String,
    /// SPDX expression or `LicenseRef-*` identifier.
    pub license: String,
    /// Asset source or standard name.
    pub source: String,
    /// Redistribution policy.
    pub redistribution: RedistributionStatus,
    /// Digest of bundled bytes; absent only for built-in assets.
    pub digest: Option<Sha256Digest>,
}

/// Non-overlapping corpus partition.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum Partition {
    /// Training or broad exploratory cases.
    Train,
    /// Development-time parameter selection cases.
    Tuning,
    /// Sealed evaluation cases not used for optimization.
    HeldOut,
    /// Purpose-built deterministic regression cases.
    Regression,
}

/// Deterministic visual/source degradation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Degradation {
    /// Native text PDF.
    Native,
    /// Native PDF with page rotation.
    Rotated {
        /// Clockwise degrees; currently 90, 180, or 270.
        degrees: i64,
    },
    /// Raster-only PDF with deterministic RGB perturbation.
    Scan {
        /// Raster resolution.
        dpi: u32,
        /// Fraction of channel values perturbed, in thousandths.
        noise_per_mille: u16,
    },
}

/// Semantic block category.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum BlockKind {
    /// Heading text.
    Heading,
    /// Paragraph text.
    Paragraph,
    /// Table region.
    Table,
    /// Formula region.
    Formula,
}

/// One truth region and rendered line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BlockSpec {
    /// Stable block-local identifier.
    pub id: String,
    /// Semantic category.
    pub kind: BlockKind,
    /// Expected Unicode text.
    pub text: String,
    /// Font size in points.
    pub font_size: u32,
    /// Left PDF coordinate.
    pub x: f64,
    /// Baseline PDF coordinate.
    pub y: f64,
    /// Approximate truth width.
    pub width: f64,
    /// Approximate truth height.
    pub height: f64,
}

/// Table semantic truth associated with one spatial block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TableTruth {
    /// Associated region ID.
    pub region_id: String,
    /// Rectangular cell grid.
    pub cells: Vec<Vec<String>>,
}

/// Formula semantic truth associated with one spatial block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FormulaTruth {
    /// Associated region ID.
    pub region_id: String,
    /// Canonical expected LaTeX.
    pub latex: String,
}

/// One requested generated case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CaseSpec {
    /// Human-readable stable name.
    pub name: String,
    /// Non-overlapping benchmark partition.
    pub partition: Partition,
    /// Deterministic degradation.
    pub degradation: Degradation,
    /// Ordered semantic blocks.
    pub blocks: Vec<BlockSpec>,
    /// Directed reading-order edges using block IDs.
    #[serde(default)]
    pub reading_order: Vec<[String; 2]>,
    /// Table semantic truth.
    #[serde(default)]
    pub tables: Vec<TableTruth>,
    /// Formula semantic truth.
    #[serde(default)]
    pub formulas: Vec<FormulaTruth>,
}

/// Versioned foundry input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FoundrySpec {
    /// Generator-input schema.
    pub schema_version: SchemaVersion,
    /// Global deterministic seed.
    pub seed: u64,
    /// Fonts and assets used by every case.
    pub assets: Vec<AssetSpec>,
    /// Generated cases.
    pub cases: Vec<CaseSpec>,
}

/// One generated corpus case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CorpusCase {
    /// Content-derived case ID.
    pub id: String,
    /// Identity independent of partition and degradation.
    pub case_identity: Sha256Digest,
    /// Declared partition.
    pub partition: Partition,
    /// Relative PDF path.
    pub document: String,
    /// Relative truth path.
    pub truth: String,
    /// Exact PDF digest.
    pub document_digest: Sha256Digest,
    /// Exact truth JSON digest.
    pub truth_digest: Sha256Digest,
    /// Truth categories present in the case.
    pub categories: BTreeSet<BlockKind>,
}

/// Reproducible corpus manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CorpusManifest {
    /// Manifest schema.
    pub schema_version: SchemaVersion,
    /// Generator implementation identity.
    pub generator_version: String,
    /// Digest of the exact input specification.
    pub spec_digest: Sha256Digest,
    /// Seed copied from the input specification.
    pub seed: u64,
    /// Explicit asset provenance.
    pub assets: Vec<AssetSpec>,
    /// Generated cases.
    pub cases: Vec<CorpusCase>,
    /// Digest over generator, spec, assets, case IDs, and artifact digests.
    pub corpus_digest: Sha256Digest,
}

/// Truth rectangle in PDF points.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TruthRect {
    /// Left coordinate.
    pub x: f64,
    /// Bottom coordinate.
    pub y: f64,
    /// Width.
    pub width: f64,
    /// Height.
    pub height: f64,
}

/// One spatial semantic truth region.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TruthRegion {
    /// Region identity.
    pub id: String,
    /// Semantic category.
    pub kind: BlockKind,
    /// Expected text.
    pub text: String,
    /// PDF-point geometry.
    pub rect: TruthRect,
}

/// Generator provenance retained with truth.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TruthProvenance {
    /// Generator identity.
    pub generator_version: String,
    /// Case-specific seed.
    pub seed: u64,
    /// Applied degradation.
    pub degradation: Degradation,
    /// Asset IDs used.
    pub assets: Vec<String>,
}

/// Complete truth for one generated case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TruthDocument {
    /// Truth schema.
    pub schema_version: SchemaVersion,
    /// Generated case ID.
    pub case_id: String,
    /// Full expected text in reading order.
    pub text: String,
    /// Spatial regions.
    pub regions: Vec<TruthRegion>,
    /// Directed reading-order edges.
    pub reading_order: Vec<[String; 2]>,
    /// Spatially associated tables.
    pub tables: Vec<TableTruth>,
    /// Spatially associated formulas.
    pub formulas: Vec<FormulaTruth>,
    /// Exact generation provenance.
    pub provenance: TruthProvenance,
}

/// Foundry validation or generation failure.
#[derive(Debug, Error)]
pub enum FoundryError {
    /// Input does not satisfy generation invariants.
    #[error("invalid foundry specification: {0}")]
    Invalid(String),
    /// Serialization failed.
    #[error("serialize foundry artifact: {0}")]
    Serialize(#[from] serde_json::Error),
    /// PDF construction or rasterization failed.
    #[error("construct foundry PDF: {0}")]
    Pdf(String),
    /// File operation failed.
    #[error("foundry file operation at {path:?}: {source}")]
    Io {
        /// Affected path.
        path: PathBuf,
        /// Underlying error.
        source: std::io::Error,
    },
}

/// Validates and atomically generates a complete corpus directory.
pub fn generate(spec: &FoundrySpec, output: &Path) -> Result<CorpusManifest, FoundryError> {
    validate_spec(spec)?;
    if output.exists() {
        return Err(FoundryError::Invalid(format!(
            "output directory {output:?} already exists"
        )));
    }
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|source| io_error(parent, source))?;
    let staging = parent.join(format!(".foundry-{}", Uuid::new_v4().simple()));
    fs::create_dir_all(staging.join("documents"))
        .and_then(|_| fs::create_dir_all(staging.join("truth")))
        .map_err(|source| io_error(&staging, source))?;
    let result = generate_into(spec, &staging).and_then(|manifest| {
        fs::rename(&staging, output).map_err(|source| io_error(output, source))?;
        Ok(manifest)
    });
    if result.is_err() && staging.exists() {
        let _ = fs::remove_dir_all(staging);
    }
    result
}

fn generate_into(spec: &FoundrySpec, root: &Path) -> Result<CorpusManifest, FoundryError> {
    let spec_digest = Sha256Digest::of_bytes(&serde_json::to_vec(spec)?);
    let mut cases = Vec::with_capacity(spec.cases.len());
    for (index, case) in spec.cases.iter().enumerate() {
        let identity = case_identity(case)?;
        let case_seed = derive_seed(spec.seed, index as u64, identity);
        let case_id = format!("case_{}", &identity.to_string()[..24]);
        let native = native_pdf(&case.blocks, rotation(&case.degradation))?;
        let document_bytes = match &case.degradation {
            Degradation::Native | Degradation::Rotated { .. } => native,
            Degradation::Scan {
                dpi,
                noise_per_mille,
            } => raster_only_pdf(native, *dpi, *noise_per_mille, case_seed)?,
        };
        let truth = truth_document(spec, case, &case_id, case_seed);
        let mut truth_bytes = serde_json::to_vec_pretty(&truth)?;
        truth_bytes.push(b'\n');
        let document_path = format!("documents/{case_id}.pdf");
        let truth_path = format!("truth/{case_id}.json");
        write(root.join(&document_path), &document_bytes)?;
        write(root.join(&truth_path), &truth_bytes)?;
        cases.push(CorpusCase {
            id: case_id,
            case_identity: identity,
            partition: case.partition,
            document: document_path,
            truth: truth_path,
            document_digest: Sha256Digest::of_bytes(&document_bytes),
            truth_digest: Sha256Digest::of_bytes(&truth_bytes),
            categories: case.blocks.iter().map(|block| block.kind).collect(),
        });
    }
    let identity = serde_json::to_vec(&(
        GENERATOR_VERSION,
        spec_digest,
        spec.seed,
        &spec.assets,
        &cases,
    ))?;
    let manifest = CorpusManifest {
        schema_version: CURRENT_SCHEMA_VERSION,
        generator_version: GENERATOR_VERSION.into(),
        spec_digest,
        seed: spec.seed,
        assets: spec.assets.clone(),
        cases,
        corpus_digest: Sha256Digest::of_bytes(&identity),
    };
    let mut bytes = serde_json::to_vec_pretty(&manifest)?;
    bytes.push(b'\n');
    write(root.join("manifest.json"), &bytes)?;
    Ok(manifest)
}

/// Validates a generated manifest and every referenced digest offline.
pub fn verify(root: &Path, manifest: &CorpusManifest) -> Result<(), FoundryError> {
    if manifest.schema_version.major != CURRENT_SCHEMA_VERSION.major
        || manifest.generator_version.trim().is_empty()
        || manifest.assets.is_empty()
    {
        return Err(FoundryError::Invalid(
            "manifest schema, generator, and assets must be supported and nonempty".into(),
        ));
    }
    validate_assets(&manifest.assets)?;
    if manifest.cases.is_empty() {
        return Err(FoundryError::Invalid("corpus contains no cases".into()));
    }
    let mut identities = BTreeSet::new();
    let mut ids = BTreeSet::new();
    for case in &manifest.cases {
        if !identities.insert(case.case_identity) || !ids.insert(&case.id) {
            return Err(FoundryError::Invalid(
                "case identity overlaps across corpus partitions".into(),
            ));
        }
        if case.id.trim().is_empty() || case.categories.is_empty() {
            return Err(FoundryError::Invalid(
                "corpus cases require IDs and truth categories".into(),
            ));
        }
        read_relative_artifact(root, &case.document, case.document_digest)?;
        let truth_bytes = read_relative_artifact(root, &case.truth, case.truth_digest)?;
        let truth: TruthDocument = serde_json::from_slice(&truth_bytes)?;
        validate_truth(case, &truth, &manifest.generator_version)?;
    }
    let identity = serde_json::to_vec(&(
        manifest.generator_version.as_str(),
        manifest.spec_digest,
        manifest.seed,
        &manifest.assets,
        &manifest.cases,
    ))?;
    if Sha256Digest::of_bytes(&identity) != manifest.corpus_digest {
        return Err(FoundryError::Invalid(
            "corpus identity digest mismatch".into(),
        ));
    }
    Ok(())
}

fn validate_spec(spec: &FoundrySpec) -> Result<(), FoundryError> {
    if spec.schema_version.major != CURRENT_SCHEMA_VERSION.major {
        return Err(FoundryError::Invalid(
            "unsupported foundry schema major".into(),
        ));
    }
    if spec.assets.is_empty() || spec.cases.is_empty() {
        return Err(FoundryError::Invalid(
            "assets and cases must both be nonempty".into(),
        ));
    }
    validate_assets(&spec.assets)?;
    let mut identities = BTreeMap::new();
    for case in &spec.cases {
        validate_case(case)?;
        let identity = case_identity(case)?;
        if let Some(partition) = identities.insert(identity, case.partition) {
            return Err(FoundryError::Invalid(format!(
                "case identity occurs in both {partition:?} and {:?}",
                case.partition
            )));
        }
    }
    Ok(())
}

fn validate_assets(assets: &[AssetSpec]) -> Result<(), FoundryError> {
    let asset_ids: BTreeSet<_> = assets.iter().map(|asset| asset.id.as_str()).collect();
    if asset_ids.len() != assets.len()
        || assets.iter().any(|asset| {
            asset.id.trim().is_empty()
                || asset.license.trim().is_empty()
                || asset.source.trim().is_empty()
                || (asset.redistribution != RedistributionStatus::BuiltIn && asset.digest.is_none())
        })
    {
        return Err(FoundryError::Invalid(
            "asset IDs, licenses, sources, and digest policies must be explicit".into(),
        ));
    }
    Ok(())
}

fn validate_truth(
    case: &CorpusCase,
    truth: &TruthDocument,
    generator_version: &str,
) -> Result<(), FoundryError> {
    let region_ids: BTreeSet<_> = truth
        .regions
        .iter()
        .map(|region| region.id.as_str())
        .collect();
    let categories: BTreeSet<_> = truth.regions.iter().map(|region| region.kind).collect();
    if truth.schema_version.major != CURRENT_SCHEMA_VERSION.major
        || truth.case_id != case.id
        || truth.text.trim().is_empty()
        || truth.regions.is_empty()
        || region_ids.len() != truth.regions.len()
        || categories != case.categories
        || truth.provenance.generator_version != generator_version
        || truth.regions.iter().any(|region| {
            region.id.trim().is_empty()
                || region.text.trim().is_empty()
                || ![
                    region.rect.x,
                    region.rect.y,
                    region.rect.width,
                    region.rect.height,
                ]
                .into_iter()
                .all(f64::is_finite)
                || region.rect.width <= 0.0
                || region.rect.height <= 0.0
        })
        || truth.reading_order.iter().any(|edge| {
            !region_ids.contains(edge[0].as_str()) || !region_ids.contains(edge[1].as_str())
        })
        || truth.tables.iter().any(|table| {
            !region_ids.contains(table.region_id.as_str()) || !rectangular(&table.cells)
        })
        || truth.formulas.iter().any(|formula| {
            !region_ids.contains(formula.region_id.as_str()) || formula.latex.trim().is_empty()
        })
    {
        return Err(FoundryError::Invalid(format!(
            "truth contract is invalid for case {}",
            case.id
        )));
    }
    Ok(())
}

fn validate_case(case: &CaseSpec) -> Result<(), FoundryError> {
    if case.name.trim().is_empty() || case.blocks.is_empty() {
        return Err(FoundryError::Invalid(
            "case name and blocks must be nonempty".into(),
        ));
    }
    match case.degradation {
        Degradation::Rotated { degrees } if !matches!(degrees, 90 | 180 | 270) => {
            return Err(FoundryError::Invalid(
                "rotation must be 90, 180, or 270 degrees".into(),
            ));
        }
        Degradation::Scan {
            dpi,
            noise_per_mille,
        } if !(72..=300).contains(&dpi) || noise_per_mille > 1000 => {
            return Err(FoundryError::Invalid(
                "scan DPI must be 72..=300 and noise <= 1000 per mille".into(),
            ));
        }
        _ => {}
    }
    let ids: BTreeSet<_> = case.blocks.iter().map(|block| block.id.as_str()).collect();
    if ids.len() != case.blocks.len()
        || case.blocks.iter().any(|block| {
            block.id.trim().is_empty()
                || block.text.trim().is_empty()
                || block.font_size == 0
                || ![block.x, block.y, block.width, block.height]
                    .into_iter()
                    .all(f64::is_finite)
                || block.width <= 0.0
                || block.height <= 0.0
        })
    {
        return Err(FoundryError::Invalid(
            "block IDs, text, font, and geometry must be valid".into(),
        ));
    }
    if case
        .reading_order
        .iter()
        .any(|edge| !ids.contains(edge[0].as_str()) || !ids.contains(edge[1].as_str()))
        || case
            .tables
            .iter()
            .any(|table| !ids.contains(table.region_id.as_str()) || !rectangular(&table.cells))
        || case.formulas.iter().any(|formula| {
            !ids.contains(formula.region_id.as_str()) || formula.latex.trim().is_empty()
        })
    {
        return Err(FoundryError::Invalid(
            "reading-order, table, or formula truth references invalid regions".into(),
        ));
    }
    Ok(())
}

fn rectangular(cells: &[Vec<String>]) -> bool {
    let Some(width) = cells.first().map(Vec::len) else {
        return false;
    };
    width > 0 && cells.iter().all(|row| row.len() == width)
}

fn case_identity(case: &CaseSpec) -> Result<Sha256Digest, FoundryError> {
    Ok(Sha256Digest::of_bytes(&serde_json::to_vec(&(
        &case.name,
        &case.blocks,
        &case.reading_order,
        &case.tables,
        &case.formulas,
    ))?))
}

fn derive_seed(seed: u64, index: u64, identity: Sha256Digest) -> u64 {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&seed.to_be_bytes());
    bytes.extend_from_slice(&index.to_be_bytes());
    bytes.extend_from_slice(identity.as_bytes());
    let digest = Sha256Digest::of_bytes(&bytes);
    u64::from_be_bytes(digest.as_bytes()[..8].try_into().expect("eight bytes"))
}

fn truth_document(spec: &FoundrySpec, case: &CaseSpec, case_id: &str, seed: u64) -> TruthDocument {
    TruthDocument {
        schema_version: CURRENT_SCHEMA_VERSION,
        case_id: case_id.into(),
        text: case
            .blocks
            .iter()
            .map(|block| block.text.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        regions: case
            .blocks
            .iter()
            .map(|block| TruthRegion {
                id: block.id.clone(),
                kind: block.kind,
                text: block.text.clone(),
                rect: TruthRect {
                    x: block.x,
                    y: block.y,
                    width: block.width,
                    height: block.height,
                },
            })
            .collect(),
        reading_order: case.reading_order.clone(),
        tables: case.tables.clone(),
        formulas: case.formulas.clone(),
        provenance: TruthProvenance {
            generator_version: GENERATOR_VERSION.into(),
            seed,
            degradation: case.degradation.clone(),
            assets: spec.assets.iter().map(|asset| asset.id.clone()).collect(),
        },
    }
}

fn rotation(degradation: &Degradation) -> Option<i64> {
    match degradation {
        Degradation::Rotated { degrees } => Some(*degrees),
        Degradation::Native | Degradation::Scan { .. } => None,
    }
}

fn native_pdf(blocks: &[BlockSpec], rotation: Option<i64>) -> Result<Vec<u8>, FoundryError> {
    build_pdf(blocks, None, rotation)
}

fn raster_only_pdf(
    native: Vec<u8>,
    dpi: u32,
    noise_per_mille: u16,
    seed: u64,
) -> Result<Vec<u8>, FoundryError> {
    let pdf = PdfDocument::from_bytes(native, PdfLimits::default())
        .map_err(|error| FoundryError::Pdf(error.to_string()))?;
    let raster = pdf
        .render_page(0, dpi)
        .map_err(|error| FoundryError::Pdf(error.to_string()))?;
    let mut rgb: Vec<_> = raster
        .rgba
        .chunks_exact(4)
        .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect();
    apply_noise(&mut rgb, noise_per_mille, seed);
    build_pdf(&[], Some((&rgb, raster.width, raster.height)), None)
}

fn apply_noise(bytes: &mut [u8], per_mille: u16, mut state: u64) {
    if per_mille == 0 {
        return;
    }
    for byte in bytes {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        if state % 1000 < u64::from(per_mille) {
            let delta = ((state >> 32) as u8 % 17).saturating_add(1);
            *byte = if state & 1 == 0 {
                byte.saturating_add(delta)
            } else {
                byte.saturating_sub(delta)
            };
        }
    }
}

fn build_pdf(
    blocks: &[BlockSpec],
    image: Option<(&[u8], u32, u32)>,
    rotation: Option<i64>,
) -> Result<Vec<u8>, FoundryError> {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let font_id = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let image_id = image.map(|(rgb, width, height)| {
        let mut stream = Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => i64::from(width),
                "Height" => i64::from(height),
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
            },
            rgb.to_vec(),
        );
        stream.compress().expect("deterministic image compression");
        document.add_object(stream)
    });
    let mut operations = Vec::new();
    if image_id.is_some() {
        operations.extend([
            Operation::new("q", vec![]),
            Operation::new(
                "cm",
                vec![
                    PAGE_WIDTH.into(),
                    0.into(),
                    0.into(),
                    PAGE_HEIGHT.into(),
                    0.into(),
                    0.into(),
                ],
            ),
            Operation::new("Do", vec![Object::Name(b"Scan".to_vec())]),
            Operation::new("Q", vec![]),
        ]);
    }
    for block in blocks {
        operations.extend([
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), i64::from(block.font_size).into()]),
            Operation::new("Td", vec![block.x.into(), block.y.into()]),
            Operation::new("Tj", vec![Object::string_literal(block.text.as_str())]),
            Operation::new("ET", vec![]),
        ]);
    }
    let content = Content { operations };
    let content_id = document.add_object(Stream::new(
        dictionary! {},
        content
            .encode()
            .map_err(|error| FoundryError::Pdf(error.to_string()))?,
    ));
    let mut resources = dictionary! { "Font" => dictionary! { "F1" => font_id } };
    if let Some(image_id) = image_id {
        resources.set("XObject", dictionary! { "Scan" => image_id });
    }
    let resources_id = document.add_object(resources);
    let mut page = dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
    };
    if let Some(rotation) = rotation {
        page.set("Rotate", rotation);
    }
    let page_id = document.add_object(page);
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), PAGE_WIDTH.into(), PAGE_HEIGHT.into()],
        }),
    );
    let catalog_id = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    document.trailer.set("Root", catalog_id);
    document.compress();
    let mut bytes = Vec::new();
    document
        .save_to(&mut bytes)
        .map_err(|error| FoundryError::Pdf(error.to_string()))?;
    Ok(bytes)
}

fn read_relative_artifact(
    root: &Path,
    relative: &str,
    digest: Sha256Digest,
) -> Result<Vec<u8>, FoundryError> {
    if relative.starts_with('/')
        || relative.contains("..")
        || relative.contains('\\')
        || relative.split('/').any(str::is_empty)
    {
        return Err(FoundryError::Invalid(
            "manifest artifact path is not normalized and relative".into(),
        ));
    }
    let path = root.join(relative);
    let bytes = fs::read(&path).map_err(|source| io_error(&path, source))?;
    if Sha256Digest::of_bytes(&bytes) != digest {
        return Err(FoundryError::Invalid(format!(
            "artifact digest mismatch for {relative}"
        )));
    }
    Ok(bytes)
}

fn write(path: PathBuf, bytes: &[u8]) -> Result<(), FoundryError> {
    fs::write(&path, bytes).map_err(|source| io_error(&path, source))
}

fn io_error(path: &Path, source: std::io::Error) -> FoundryError {
    FoundryError::Io {
        path: path.to_owned(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> FoundrySpec {
        FoundrySpec {
            schema_version: CURRENT_SCHEMA_VERSION,
            seed: 42,
            assets: vec![AssetSpec {
                id: "pdf.helvetica".into(),
                license: "ISO-32000-2".into(),
                source: "PDF standard 14 font".into(),
                redistribution: RedistributionStatus::BuiltIn,
                digest: None,
            }],
            cases: vec![CaseSpec {
                name: "fixture".into(),
                partition: Partition::Train,
                degradation: Degradation::Scan {
                    dpi: 96,
                    noise_per_mille: 5,
                },
                blocks: vec![BlockSpec {
                    id: "heading".into(),
                    kind: BlockKind::Heading,
                    text: "DETERMINISTIC CORPUS".into(),
                    font_size: 20,
                    x: 72.0,
                    y: 700.0,
                    width: 300.0,
                    height: 24.0,
                }],
                reading_order: Vec::new(),
                tables: Vec::new(),
                formulas: Vec::new(),
            }],
        }
    }

    #[test]
    fn corpus_is_byte_reproducible_and_verifies_offline() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let first_root = first.path().join("corpus");
        let second_root = second.path().join("corpus");
        let first_manifest = generate(&spec(), &first_root).unwrap();
        let second_manifest = generate(&spec(), &second_root).unwrap();
        assert_eq!(first_manifest, second_manifest);
        assert_eq!(
            fs::read(first_root.join(&first_manifest.cases[0].document)).unwrap(),
            fs::read(second_root.join(&second_manifest.cases[0].document)).unwrap()
        );
        verify(&first_root, &first_manifest).unwrap();
    }

    #[test]
    fn case_identity_cannot_cross_partitions() {
        let mut spec = spec();
        let mut duplicate = spec.cases[0].clone();
        duplicate.partition = Partition::HeldOut;
        duplicate.degradation = Degradation::Native;
        spec.cases.push(duplicate);
        assert!(matches!(
            generate(&spec, Path::new("unused")),
            Err(FoundryError::Invalid(_))
        ));
    }

    #[test]
    fn table_and_formula_truth_require_spatial_regions() {
        let mut spec = spec();
        spec.cases[0].tables.push(TableTruth {
            region_id: "missing".into(),
            cells: vec![vec!["x".into()]],
        });
        assert!(validate_spec(&spec).is_err());
    }

    #[test]
    fn verifier_rejects_digest_consistent_empty_truth() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("corpus");
        let mut manifest = generate(&spec(), &root).unwrap();
        let truth_path = root.join(&manifest.cases[0].truth);
        let mut truth: TruthDocument =
            serde_json::from_slice(&fs::read(&truth_path).unwrap()).unwrap();
        truth.text.clear();
        let mut bytes = serde_json::to_vec_pretty(&truth).unwrap();
        bytes.push(b'\n');
        fs::write(&truth_path, &bytes).unwrap();
        manifest.cases[0].truth_digest = Sha256Digest::of_bytes(&bytes);
        manifest.corpus_digest = Sha256Digest::of_bytes(
            &serde_json::to_vec(&(
                manifest.generator_version.as_str(),
                manifest.spec_digest,
                manifest.seed,
                &manifest.assets,
                &manifest.cases,
            ))
            .unwrap(),
        );
        assert!(verify(&root, &manifest).is_err());
    }
}
