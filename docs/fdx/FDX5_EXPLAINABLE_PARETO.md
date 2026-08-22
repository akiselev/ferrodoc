# FDX5 explainable local Pareto planning

Status: implemented FP5 reference contract.

Ferrodoc can compare admissible ways to satisfy the same capability goal without executing an
engine. It first applies the existing hard policy gates (placement, device compatibility, model,
offline/privacy, deadline, remote cost, RAM, and VRAM). Unknown values for a hard numeric bound
remain rejected unless the caller explicitly opts into guarded execution; the `low-vram` profile
therefore remains an honest 2 GiB VRAM ceiling.

Each returned `EnrichmentCandidatePlan` contains:

- a deterministic semantic `plan_id`, pinned input state, ordered scoped invocations, engine and
  placement identities;
- per-stage capability-gain, success-probability, quality, CPU/GPU time, read-byte, and write-byte
  intervals with source, confidence, and optional benchmark-report digest;
- aggregate intervals, prerequisites, verified durable-cache state, and explicit escalation
  reasons for whole-document work, accelerators, network-capable engines, cold cache,
  prerequisites, unknown estimates, and heuristic evidence.

Hard remote-cost and deadline limits apply to the checked sum for the complete sequential plan,
not independently to each stage. Overflow refuses the plan. Peak RAM/VRAM remains a peak bound
because the reference executor runs invocations sequentially.

Outcome estimates use integer basis points and resources use integer native units. Plan identity
excludes the outer request ID, estimates, cache state, resource observations, and queue priority.
It includes the source, logical input state, schema, stage/build/model, engine/version,
backend/device, explicitly normalized stage parameters, seed, and semantic scope. Reserved
Ferrodoc parameters cannot be supplied as stage configuration. Thus changed evidence or an
execution choice changes identity, while refreshed measurements do not.

## Frontier semantics

Every placement that passes hard admission is retained for comparison. A plan dominates another
only when every known benefit interval is conservatively no worse, every known cost interval is
conservatively no worse, and at least one dimension is strictly better. Overlapping intervals or
an unknown dimension make dominance indeterminate, preserving the tradeoff. No scalar utility
score or hidden tie-breaker selects a winner. Candidate explosion fails explicitly at the bounded
64-plan contract rather than silently truncating the frontier.

Only integer planning intervals determine quality dominance. A floating-point quality value may
remain in an engine's raw resource diagnostic, but it cannot determine the replayable frontier.
Integer resource sums use checked arithmetic, and exact ties remain separate Pareto alternatives.

Stages declare whether they honor the requested page-qualified scope or require whole-document
execution. Whole-document invocations are deduplicated across narrow atomic goals and reported as
an escalation. This makes targeted and whole-document alternatives directly comparable.

## Evidence and ownership limits

The integration oracle uses purpose-built minimized observations whose method is explicitly
`controlled_minimized_fixture_not_corpus_quality`. It proves fixed-point aggregation, source and
report-digest retention, Pareto behavior, and deterministic identity. It is not evidence of
representative datasheet quality, latency, RAM/VRAM, or cost. Those claims require qualified-engine
measurements over a licensed, content-verified corpus and relevant hardware classes.

Ferrodoc owns this local capability/engine frontier. It does not assign domain value, corpus reuse
value, publisher authority, fairness, queue priority, or a global winner. Those scheduling and
portfolio decisions remain Foundry responsibilities.
