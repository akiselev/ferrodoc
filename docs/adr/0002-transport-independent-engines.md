# ADR 0002: transport-independent engines

Status: accepted

Date: 2026-08-19

## Context

Heavy OCR and model runtimes benefit from process isolation, while applications and tests also need direct embedded calls. A Rust dynamic-library ABI would introduce an unstable compatibility boundary, and an async engine trait would leak orchestration runtime choices into model integrations that are predominantly blocking.

## Decision

Define one synchronous `Engine: Send` trait in `ferrodoc-engine-api`. It receives serializable semantic requests plus a non-serializable execution context for cancellation, deadlines, scoped blobs, and structured tracing. Embedded registration calls the trait directly. Process wrappers translate versioned `ferrodoc-protocol` messages to the same trait without changing engine behavior.

Engine request schemas contain only opaque blob tokens and checked ranges. Host filesystem paths and unrestricted network handles are not part of the semantic API.

## Consequences

Engine implementations remain reusable in embedded and isolated modes. The runtime owns worker threads, processes, resource leases, restart policy, and async integration. Process parity and failure behavior become testable without defining a stable Rust ABI.
