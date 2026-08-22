//! Deterministic targeted reconstruction for simple delimiter-separated tables.
//!
//! This bounded engine is an evidence-contract oracle, not a general table model. It recognizes
//! consistent, nonempty pipe-delimited rows in existing target-region text evidence and cites the
//! exact UTF-8 byte range used for every cell.

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
        Ok(vec![EngineCandidate {
            engine_id: ENGINE_ID.into(),
            backend: BackendId::new("rules").expect("static backend"),
            device: DeviceId::new(DeviceKind::Cpu, None).expect("static device"),
            resources: ResourceEstimate {
                peak_ram: Estimate::Known(Bytes::new(8 * Bytes::MIB)),
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
        let input_digest = request
            .input
            .expected_digest
            .unwrap_or_else(|| Sha256Digest::of_bytes(&source_pdf));
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
        for source in sources {
            if let Some((rows, columns, cells)) = parse_table(&source)? {
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
                        serde_json::json!("nonempty_pipe_rows_v1"),
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
    let mut parsed_rows = Vec::new();
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

fn parse_row(line: &str, line_start: usize) -> Result<Option<Vec<ParsedCell>>, EngineError> {
    let delimiters = line
        .match_indices('|')
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if delimiters.is_empty() {
        return Ok(None);
    }
    let mut boundaries = Vec::with_capacity(delimiters.len() + 2);
    boundaries.push(0);
    boundaries.extend(delimiters.iter().map(|index| index + 1));
    boundaries.push(line.len() + 1);
    let mut cells = Vec::new();
    for pair in boundaries.windows(2) {
        let raw_start = pair[0];
        let raw_end = pair[1] - 1;
        let raw = &line[raw_start..raw_end];
        if raw.trim().is_empty() {
            if (raw_start == 0 && delimiters.first() == Some(&0))
                || (raw_end == line.len() && delimiters.last() == Some(&(line.len() - 1)))
            {
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
}
