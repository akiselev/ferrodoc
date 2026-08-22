use std::collections::{BTreeMap, BTreeSet};

use ferrodoc_core::{
    BlobId, BlobRange, Capability, CoordinateSpace, CoordinateTransform, EvidenceId, MediaType,
    PageId, PageRect, Rect, RegionId, RequestId, ScopedBlob, Sha256Digest, Unit,
};
use ferrodoc_engine_api::{
    EngineRequest, SOURCE_TEXT_EVIDENCE_PARAMETER, SourceTextEvidence, conformance,
};
use ferrodoc_ir::{EvidenceContent, GeometryQuality, PageRegionRef, RefinementScope};
use ferrodoc_table_rulebased::RuleBasedTableEngine;
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    source_text: String,
    rows: u32,
    columns: u32,
    cells: Vec<ExpectedCell>,
}

#[derive(Deserialize)]
struct ExpectedCell {
    row: u32,
    column: u32,
    text: String,
    start: u32,
    end: u32,
}

#[test]
fn table_engine_passes_common_conformance() {
    let fixture = fixture();
    let bytes = b"%PDF-minimized-table-contract".to_vec();
    conformance::run(
        &mut RuleBasedTableEngine::new(),
        request(&bytes, &fixture.source_text),
        bytes,
        &conformance::unknown_inventory(),
    )
    .unwrap();
}

#[test]
fn reconstructs_fixture_with_exact_resolvable_spans_and_honest_geometry() {
    use ferrodoc_engine_api::{
        BlobResolver, CancellationToken, Engine, ExecutionContext, TraceSink,
    };

    struct Resolver(Vec<u8>);
    impl BlobResolver for Resolver {
        fn resolve(&self, _blob: &ScopedBlob) -> Result<Vec<u8>, ferrodoc_engine_api::EngineError> {
            Ok(self.0.clone())
        }
    }
    struct Trace;
    impl TraceSink for Trace {
        fn event(&self, _code: &str, _fields: &BTreeMap<String, String>) {}
    }

    let fixture = fixture();
    let bytes = b"%PDF-minimized-table-contract".to_vec();
    let request = request(&bytes, &fixture.source_text);
    let resolver = Resolver(bytes);
    let response = RuleBasedTableEngine::new()
        .execute(
            request,
            &ExecutionContext {
                cancellation: CancellationToken::default(),
                deadline: None,
                blobs: &resolver,
                trace: &Trace,
            },
        )
        .unwrap();
    assert_eq!(response.evidence.len(), 1);
    let evidence = &response.evidence[0];
    assert_eq!(evidence.geometry_quality, GeometryQuality::Region);
    let EvidenceContent::Table {
        rows,
        columns,
        cells,
    } = &evidence.content
    else {
        panic!("expected table evidence")
    };
    assert_eq!((*rows, *columns), (fixture.rows, fixture.columns));
    assert_eq!(cells.len(), fixture.cells.len());
    for (cell, expected) in cells.iter().zip(fixture.cells) {
        assert_eq!((cell.row, cell.column), (expected.row, expected.column));
        assert_eq!(cell.text, expected.text);
        assert_eq!(cell.geometry_quality, GeometryQuality::Region);
        assert_eq!(cell.source_spans.len(), 1);
        let span = &cell.source_spans[0];
        assert_eq!((span.start, span.end), (expected.start, expected.end));
        assert_eq!(
            fixture
                .source_text
                .get(span.start as usize..span.end as usize),
            Some(cell.text.as_str())
        );
    }
}

fn fixture() -> Fixture {
    serde_json::from_str(include_str!("../../../fixtures/table/pipe-table-v1.json")).unwrap()
}

fn request(bytes: &[u8], text: &str) -> EngineRequest {
    let page_id = PageId::derive(&[b"table-page"]);
    let region_id = RegionId::derive(&[b"table-region"]);
    let geometry = PageRect {
        page_index: 0,
        rect: Rect::new(10.0, 20.0, 300.0, 120.0, CoordinateSpace::Pdf, Unit::Point).unwrap(),
        source_transform: CoordinateTransform::IDENTITY,
    };
    EngineRequest {
        request_id: RequestId::derive(&[b"table-conformance"]),
        capability: Capability::TableRecognize,
        input: ScopedBlob {
            id: BlobId::new("table-source-pdf").unwrap(),
            range: BlobRange::new(0, bytes.len() as u64).unwrap(),
            media_type: MediaType::new("application/pdf").unwrap(),
            expected_digest: Some(Sha256Digest::of_bytes(bytes)),
        },
        page_index: Some(0),
        scope: Some(RefinementScope::Regions {
            regions: BTreeSet::from([PageRegionRef { page_id, region_id }]),
        }),
        parameters: BTreeMap::from([(
            SOURCE_TEXT_EVIDENCE_PARAMETER.into(),
            serde_json::to_value(vec![SourceTextEvidence {
                evidence_id: EvidenceId::derive(&[b"source-text"]),
                text: text.into(),
                geometry: Some(geometry),
                geometry_quality: GeometryQuality::Region,
            }])
            .unwrap(),
        )]),
        deterministic_seed: None,
        deadline: None,
    }
}
