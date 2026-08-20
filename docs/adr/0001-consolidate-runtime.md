# ADR 0001: consolidate runtime coordination

Status: accepted

Date: 2026-08-19

## Context

The imported manifest named separate planner, scheduler, model-store, router, plugin-host, cache, and pipeline packages without committed implementations. Empty package boundaries made the workspace unbuildable and would force versioning and dependency decisions before their pressures are known.

## Decision

Keep semantic and compatibility boundaries as distinct crates: core primitives, evidence IR, engine API, process protocol, PDF integration, and rendering. Begin planner, scheduler, model coordination, cache, process hosting, routing, and pipeline orchestration as modules of `ferrodoc-runtime`. Extract a module only when independent publication, reuse, platform isolation, or compile-time pressure is demonstrated.

## Consequences

The workspace is smaller and every package has a target, tests, and a present role. Runtime internals can evolve together during v0.2. A future extraction requires an ADR and measured dependency or reuse evidence rather than restoration of an old placeholder name.
