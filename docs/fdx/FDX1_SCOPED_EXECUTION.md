# FDX1 capability-scoped runtime execution

Status: FP1 implemented contract

## Request and planning contract

`EnrichmentRequest` pins exact source bytes through a `ScopedBlob`, one
`DocumentStateId`, and a nonempty list of `CapabilityGoal` values. Each goal uses the
same `RefinementScope` persisted in `EvidenceDelta`: document, explicit pages, or
explicit `PageRegionRef` values. Region IDs remain page-local. Planning validates the
materialized base document and state before candidate enumeration, including the
containing page for every region.

Page and region sets become deterministic singleton invocations. A registered
`EnrichmentStageDescriptor` declares an immutable build identity, exactly one produced capability, and every
capability that must already be complete over the same scope. The planner never hides
unrequested prerequisite execution. It returns one of:

- `AlreadySatisfied` when retained complete coverage contains every goal;
- `NoAdmissiblePlan` with stable reasons when a producer, prerequisite, model,
  placement, privacy, offline, resource, cost, or deadline gate fails;
- `CandidatePlans { pareto }` with a bounded deterministic set of admissible local
  alternatives.

FP1's frontier contains the existing conservative resource/quality estimates. The
benchmark-derived value/gain comparison surface remains FP5 work.

## Execution and delta contract

Every invocation remains an ordinary transport-independent `EngineRequest`. Its new
optional `scope` field carries the semantic scope while `page_index` remains the
engine-facing page coordinate. The runtime also includes canonical scope in normalized
parameters, so distinct page/region invocations cannot alias in the deterministic stage
cache. A semantic request ID is derived from source, stage, capability, and scope rather
than the observational parent request ID.

Execution revalidates the selected plan against the current admissible frontier, uses
the established planner, scheduler lease, cache, cancellation/deadline, and scoped-blob
resolver, and accepts either embedded engines or `ProcessEngine`. It does not expose a
host path or add network access.

FP1 emits one immutable `EvidenceDelta` per atomic invocation. Evidence-producing FP1
stages require one page-qualified region owner; the runtime rejects cross-page geometry,
writes to an untargeted region on the same page, and invalid empty or absent scopes even
for no-op results. It creates an explicit region-owned source layer when the engine
returns a new layer.
Empty discovery-style results record `candidate`, not false `complete`, coverage.
Returned deltas are applied through `materialize_from_checkpoint`; the result includes
the new content-identifiable manifest, a canonical DocumentIR conformance view, and a
checkpoint reference binding subsequent refinement to those exact DocumentIR bytes.
Resource and cache observations remain outside delta/state identity.

## Compatibility and acceptance evidence

`EngineRequest.scope` is optional only so existing protocol-v1 messages decode with
`scope = None`. New enrichment execution always supplies it. Checked-in bounded CBOR
fixtures prove both the legacy shape and a page-qualified scoped request. The process
protocol's existing frame-size, blob, timeout, cancellation, and failure bounds are
unchanged.

The FP1 regression requests `table.recognize` for two page-qualified regions whose
`RegionId` values deliberately collide across pages. It proves:

- exactly two table invocations and no implicit OCR/layout prerequisite work;
- an unrelated third page receives no evidence;
- a matching region ID on the wrong page is rejected;
- unsupported capabilities and incomplete declared prerequisites return no plan;
- completed scope coverage returns already satisfied;
- repeated execution yields identical deltas and `DocumentStateId`;
- all additions materialize through the FP0 delta/state contract.

FP1 does not claim a real table engine, survey/baseline profile, representative PDF
benchmark, durable cross-worker artifact store, or benchmark-valued Pareto planner.
Those remain FP2-FP5 deliverables.
