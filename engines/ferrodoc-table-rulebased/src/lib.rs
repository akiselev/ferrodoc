//! Deterministic targeted reconstruction for simple delimiter-separated tables.
//!
//! This bounded engine is an evidence-contract oracle, not a general table model. It recognizes
//! consistent, nonempty pipe-delimited rows in existing target-region text evidence and cites the
//! exact UTF-8 byte range used for every cell. A second deliberately narrow grammar recognizes a
//! `Name Description` header followed by one symbol and a sentence description. This covers a
//! common born-digital datasheet table fragment without claiming general visual table recovery.

use std::collections::{BTreeMap, BTreeSet};

use ferrodoc_core::{
    BackendId, Bytes, CURRENT_SCHEMA_VERSION, Capability, DeterministicProvenance, DeviceId,
    DeviceKind, Estimate, EstimateConfidence, EstimateSource, EvidenceId, LayerId, MicroUsd,
    Millis, ResourceEstimate, Sha256Digest, Stage,
};
use ferrodoc_engine_api::{
    Engine, EngineCandidate, EngineCompatibility, EngineDescriptor, EngineError,
    EngineErrorCategory, EngineRequest, EngineResponse, ExecutionContext, HardwareInventory,
    HealthReport, HealthRequest, HealthStatus, NetworkUse, SourceTextEvidence, evidence_parameters,
    source_text_evidence,
};
use ferrodoc_ir::{
    Evidence, EvidenceContent, GeometryQuality, RefinementScope, TableCell, TextSourceSpan,
};

/// Stable engine identifier.
pub const ENGINE_ID: &str = "table.rulebased";
/// Engine semantic version.
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
const MAX_ROWS: usize = 4_096;
const MAX_COLUMNS: usize = 256;
const MAX_CELLS: usize = 65_536;
const PARSER_WORKING_SET_BYTES: u64 = 32 * Bytes::MIB;
type ParsedCell = (String, usize, usize);

/// Always-available CPU engine for the bounded delimiter grammar.
pub struct RuleBasedTableEngine {
    descriptor: EngineDescriptor,
}

impl Default for RuleBasedTableEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl RuleBasedTableEngine {
    /// Constructs the deterministic engine.
    pub fn new() -> Self {
        Self {
            descriptor: EngineDescriptor {
                id: ENGINE_ID.into(),
                version: ENGINE_VERSION.into(),
                capabilities: BTreeSet::from([Capability::TableRecognize]),
                compatibility: vec![EngineCompatibility {
                    backend: BackendId::new("rules").expect("static backend"),
                    devices: BTreeSet::from([DeviceKind::Cpu]),
                }],
                deterministic: true,
                network_use: NetworkUse::None,
                max_concurrency: 64,
            },
        }
    }
}

impl Engine for RuleBasedTableEngine {
    fn descriptor(&self) -> &EngineDescriptor {
        &self.descriptor
    }

    fn health(&mut self, _request: HealthRequest) -> Result<HealthReport, EngineError> {
        Ok(HealthReport {
            status: HealthStatus::Healthy,
            dependencies: Vec::new(),
            message: "bounded delimiter table engine is ready".into(),
        })
    }

    fn estimate(
        &mut self,
        request: &EngineRequest,
        _inventory: &HardwareInventory,
    ) -> Result<Vec<EngineCandidate>, EngineError> {
        require_request(request)?;
        let _ = source_text_evidence(request)?;
        let peak_ram = Bytes::new(request.input.range.len())
            .checked_add(Bytes::new(PARSER_WORKING_SET_BYTES))
            .map_err(|_| resource("table memory estimate overflow"))?;
        Ok(vec![EngineCandidate {
            engine_id: ENGINE_ID.into(),
            backend: BackendId::new("rules").expect("static backend"),
            device: DeviceId::new(DeviceKind::Cpu, None).expect("static device"),
            resources: ResourceEstimate {
                peak_ram: Estimate::Known(peak_ram),
                warm_ram: Estimate::Known(Bytes::new(0)),
                peak_vram: Estimate::Known(Bytes::new(0)),
                warm_vram: Estimate::Known(Bytes::new(0)),
                latency: Estimate::Known(Millis::new(5)),
                remote_cost: Estimate::Known(MicroUsd::new(0)),
                quality: Estimate::Unknown,
                source: Estimate::Known(EstimateSource {
                    confidence: EstimateConfidence::Conservative,
                    method: "bounded delimiter parser envelope".into(),
                }),
            },
        }])
    }

    fn execute(
        &mut self,
        request: EngineRequest,
        context: &ExecutionContext<'_>,
    ) -> Result<EngineResponse, EngineError> {
        require_request(&request)?;
        context.checkpoint()?;
        let source_pdf = context.blobs.resolve(&request.input)?;
        let input_digest = request
            .input
            .expected_digest
            .unwrap_or_else(|| Sha256Digest::of_bytes(&source_pdf));
        drop(source_pdf);
        let sources = source_text_evidence(&request)?;
        let page_index = request.page_index.expect("validated atomic table request");
        if sources.iter().any(|source| {
            source
                .geometry
                .is_some_and(|geometry| geometry.page_index != page_index)
                || (source.geometry.is_none()
                    && source.geometry_quality != GeometryQuality::Unknown)
        }) {
            return Err(invalid(
                "source text geometry differs from its page-qualified target",
            ));
        }
        let provenance = DeterministicProvenance {
            schema_version: CURRENT_SCHEMA_VERSION,
            input_digest,
            engine_id: ENGINE_ID.into(),
            engine_version: ENGINE_VERSION.into(),
            model_digest: None,
            parameters: evidence_parameters(&request),
            stage: Stage::Layout,
        };
        let provenance_digest = provenance
            .identity_digest()
            .map_err(|error| internal(error.to_string()))?;
        let layer_id = LayerId::derive(&[provenance_digest.as_bytes()]);
        let mut evidence = Vec::new();
        let mut parsed_source_text = BTreeSet::new();
        for source in sources {
            // Reconciliation can retain byte-identical native and layout hypotheses in one region.
            // One semantic table is sufficient; preserve deterministic source order so the native,
            // honestly page-only source wins over a later coarse layout duplicate.
            if !parsed_source_text.insert(source.text.clone()) {
                continue;
            }
            if let Some((rows, columns, cells)) = parse_table(&source)? {
                let grammar = if source.text.contains('|') {
                    "nonempty_pipe_rows_v1"
                } else {
                    "name_description_fragment_v1"
                };
                evidence.push(Evidence {
                    id: EvidenceId::derive(&[
                        provenance_digest.as_bytes(),
                        source.evidence_id.as_str().as_bytes(),
                    ]),
                    layer_id: layer_id.clone(),
                    content: EvidenceContent::Table {
                        rows,
                        columns,
                        cells,
                    },
                    geometry: source.geometry,
                    geometry_quality: source.geometry_quality,
                    confidence: None,
                    provenance: provenance.clone(),
                    engine_metadata: BTreeMap::from([(
                        "grammar".into(),
                        serde_json::json!(grammar),
                    )]),
                });
            }
        }
        context.checkpoint()?;
        let recognized_hypotheses = evidence.len();
        Ok(EngineResponse {
            request_id: request.request_id,
            evidence,
            metadata: BTreeMap::from([(
                "recognized_hypotheses".into(),
                serde_json::json!(recognized_hypotheses),
            )]),
        })
    }
}

fn require_request(request: &EngineRequest) -> Result<(), EngineError> {
    if request.capability != Capability::TableRecognize {
        return Err(unsupported(
            "table rule engine only supports table recognition",
        ));
    }
    if request.input.media_type.as_str() != "application/pdf" {
        return Err(invalid("table recognition input must be application/pdf"));
    }
    if request.page_index.is_none()
        || !matches!(
            &request.scope,
            Some(RefinementScope::Regions { regions }) if regions.len() == 1
        )
    {
        return Err(invalid(
            "table recognition requires one page-qualified target region",
        ));
    }
    Ok(())
}

fn parse_table(
    source: &SourceTextEvidence,
) -> Result<Option<(u32, u32, Vec<TableCell>)>, EngineError> {
    if !source.text.contains('|') {
        return parse_name_description_fragment(source);
    }
    let mut parsed_rows = Vec::new();
    let mut parsed_cells = 0_usize;
    let mut line_start = 0_usize;
    for line_with_ending in source.text.split_inclusive('\n') {
        let line = line_with_ending
            .strip_suffix('\n')
            .unwrap_or(line_with_ending);
        let line = line.strip_suffix('\r').unwrap_or(line);
        if !line.trim().is_empty() {
            let Some(cells) = parse_row(line, line_start)? else {
                return Ok(None);
            };
            parsed_cells = parsed_cells
                .checked_add(cells.len())
                .ok_or_else(|| resource("table cell count overflow"))?;
            if parsed_rows.len() == MAX_ROWS || parsed_cells > MAX_CELLS {
                return Ok(None);
            }
            parsed_rows.push(cells);
        }
        line_start = line_start
            .checked_add(line_with_ending.len())
            .ok_or_else(|| resource("source offset overflow"))?;
    }
    if !source.text.is_empty() && !source.text.ends_with('\n') && !source.text.contains('\n') {
        // split_inclusive already visited the only line; no special handling is required.
    }
    if parsed_rows.len() < 2 || parsed_rows.len() > MAX_ROWS {
        return Ok(None);
    }
    let columns = parsed_rows[0].len();
    if !(2..=MAX_COLUMNS).contains(&columns)
        || parsed_rows.iter().any(|row| row.len() != columns)
        || parsed_rows
            .len()
            .checked_mul(columns)
            .is_none_or(|count| count > MAX_CELLS)
    {
        return Ok(None);
    }
    let rows = u32::try_from(parsed_rows.len()).expect("bounded rows");
    let mut cells = Vec::with_capacity(parsed_rows.len() * columns);
    for (row, parsed) in parsed_rows.into_iter().enumerate() {
        for (column, (text, start, end)) in parsed.into_iter().enumerate() {
            cells.push(TableCell {
                row: u32::try_from(row).map_err(|_| resource("row index overflow"))?,
                column: u32::try_from(column).map_err(|_| resource("column index overflow"))?,
                row_span: 1,
                column_span: 1,
                text,
                geometry: source.geometry,
                geometry_quality: source.geometry_quality,
                source_spans: vec![TextSourceSpan {
                    evidence_id: source.evidence_id.clone(),
                    start: u32::try_from(start).map_err(|_| resource("source offset overflow"))?,
                    end: u32::try_from(end).map_err(|_| resource("source offset overflow"))?,
                }],
            });
        }
    }
    Ok(Some((
        rows,
        u32::try_from(columns).expect("bounded columns"),
        cells,
    )))
}

fn parse_name_description_fragment(
    source: &SourceTextEvidence,
) -> Result<Option<(u32, u32, Vec<TableCell>)>, EngineError> {
    const HEADER: &str = "Name Description ";
    let Some(body) = source.text.strip_prefix(HEADER) else {
        return Ok(None);
    };
    if body.contains(['\r', '\n', '|']) || body.len() > 4 * 1_024 {
        return Ok(None);
    }
    let Some(separator) = body.find(' ') else {
        return Ok(None);
    };
    let name = &body[..separator];
    let description = body[separator + 1..].trim_end();
    if name.is_empty()
        || name.len() > 64
        || !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_/-".contains(character))
        || description.len() < 16
        || !description.ends_with(['.', '!', '?'])
    {
        return Ok(None);
    }
    let description_start = HEADER
        .len()
        .checked_add(separator)
        .and_then(|offset| offset.checked_add(1))
        .ok_or_else(|| resource("source offset overflow"))?;
    let description_end = description_start
        .checked_add(description.len())
        .ok_or_else(|| resource("source offset overflow"))?;
    let fields = [
        ("Name", 0_usize, 4_usize),
        ("Description", 5_usize, 16_usize),
        (name, HEADER.len(), HEADER.len() + separator),
        (description, description_start, description_end),
    ];
    let mut cells = Vec::with_capacity(fields.len());
    for (index, (text, start, end)) in fields.into_iter().enumerate() {
        cells.push(TableCell {
            row: u32::try_from(index / 2).expect("bounded row"),
            column: u32::try_from(index % 2).expect("bounded column"),
            row_span: 1,
            column_span: 1,
            text: text.to_owned(),
            geometry: source.geometry,
            geometry_quality: source.geometry_quality,
            source_spans: vec![TextSourceSpan {
                evidence_id: source.evidence_id.clone(),
                start: u32::try_from(start).map_err(|_| resource("source offset overflow"))?,
                end: u32::try_from(end).map_err(|_| resource("source offset overflow"))?,
            }],
        });
    }
    Ok(Some((2, 2, cells)))
}

fn parse_row(line: &str, line_start: usize) -> Result<Option<Vec<ParsedCell>>, EngineError> {
    let mut delimiters = line.match_indices('|').map(|(index, _)| index).peekable();
    if delimiters.peek().is_none() {
        return Ok(None);
    }
    let mut cells = Vec::with_capacity(MAX_COLUMNS.min(16));
    let mut raw_start = 0_usize;
    for raw_end in delimiters.chain(std::iter::once(line.len())) {
        let raw = &line[raw_start..raw_end];
        if raw.trim().is_empty() {
            if (raw_start == 0 && raw_end == 0)
                || (raw_start == line.len() && raw_end == line.len())
            {
                raw_start = raw_end.saturating_add(1);
                continue;
            }
            return Ok(None);
        }
        let leading = raw.len() - raw.trim_start().len();
        let trailing = raw.trim_end().len();
        let start = raw_start + leading;
        let end = raw_start + trailing;
        let absolute_start = line_start
            .checked_add(start)
            .ok_or_else(|| resource("source offset overflow"))?;
        let absolute_end = line_start
            .checked_add(end)
            .ok_or_else(|| resource("source offset overflow"))?;
        cells.push((line[start..end].to_owned(), absolute_start, absolute_end));
        if cells.len() > MAX_COLUMNS {
            return Ok(None);
        }
        raw_start = raw_end.saturating_add(1);
    }
    Ok((cells.len() >= 2).then_some(cells))
}

fn invalid(message: impl Into<String>) -> EngineError {
    EngineError::new(EngineErrorCategory::InvalidRequest, false, message)
}

fn unsupported(message: impl Into<String>) -> EngineError {
    EngineError::new(EngineErrorCategory::Unsupported, false, message)
}

fn resource(message: impl Into<String>) -> EngineError {
    EngineError::new(EngineErrorCategory::ResourceExhausted, false, message)
}

fn internal(message: impl Into<String>) -> EngineError {
    EngineError::new(EngineErrorCategory::Internal, false, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrodoc_ir::GeometryQuality;

    fn source(text: &str) -> SourceTextEvidence {
        SourceTextEvidence {
            evidence_id: EvidenceId::derive(&[b"parser-source"]),
            text: text.into(),
            geometry: None,
            geometry_quality: GeometryQuality::Unknown,
        }
    }

    #[test]
    fn utf8_and_crlf_offsets_resolve_exactly() {
        let source = source("Ω | 名\r\n1 | 值");
        let (_, _, cells) = parse_table(&source).unwrap().unwrap();
        assert_eq!(
            cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>(),
            ["Ω", "名", "1", "值"]
        );
        for cell in cells {
            let span = &cell.source_spans[0];
            assert_eq!(
                source.text.get(span.start as usize..span.end as usize),
                Some(cell.text.as_str())
            );
        }
    }

    #[test]
    fn inconsistent_or_empty_cells_are_not_claimed_as_tables() {
        assert!(parse_table(&source("A | B\n1 | 2 | 3")).unwrap().is_none());
        assert!(parse_table(&source("A | B\n1 | ")).unwrap().is_none());
        assert!(
            parse_table(&source("ordinary paragraph"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn bounded_name_description_fragment_has_exact_source_spans() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/table/name-description-fragment-v1.json"
        ))
        .unwrap();
        let fixture_source = source(fixture["source_text"].as_str().unwrap());
        let (rows, columns, cells) = parse_table(&fixture_source).unwrap().unwrap();
        assert_eq!((rows, columns), (2, 2));
        assert_eq!(cells.len(), 4);
        for (cell, expected) in cells.iter().zip(fixture["cells"].as_array().unwrap()) {
            assert_eq!(cell.row, expected["row"].as_u64().unwrap() as u32);
            assert_eq!(cell.column, expected["column"].as_u64().unwrap() as u32);
            assert_eq!(cell.text, expected["text"].as_str().unwrap());
            let span = &cell.source_spans[0];
            assert_eq!(span.start, expected["start"].as_u64().unwrap() as u32);
            assert_eq!(span.end, expected["end"].as_u64().unwrap() as u32);
            assert_eq!(
                fixture_source
                    .text
                    .get(span.start as usize..span.end as usize),
                Some(cell.text.as_str())
            );
        }
        assert!(
            parse_table(&source("Name Description GPIOx too short"))
                .unwrap()
                .is_none()
        );
        assert!(
            parse_table(&source(
                "Prefix Name Description GPIOx A valid-looking sentence."
            ))
            .unwrap()
            .is_none()
        );
    }

    #[test]
    fn structural_limits_refuse_before_accumulating_unbounded_rows_or_cells() {
        let overwide = std::iter::repeat_n("x", MAX_COLUMNS + 1)
            .collect::<Vec<_>>()
            .join("|");
        assert!(parse_row(&overwide, 0).unwrap().is_none());

        let too_many_rows = "a|b\n".repeat(MAX_ROWS + 1);
        assert!(parse_table(&source(&too_many_rows)).unwrap().is_none());

        let maximum_width = std::iter::repeat_n("x", MAX_COLUMNS)
            .collect::<Vec<_>>()
            .join("|");
        let too_many_cells = format!("{}\n", maximum_width).repeat(MAX_CELLS / MAX_COLUMNS + 1);
        assert!(parse_table(&source(&too_many_cells)).unwrap().is_none());
    }
}
