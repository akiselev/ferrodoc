# Known limitations

- The repository currently exposes only the `ferrodoc-core` library. It cannot inspect or convert PDFs.
- The existing core types are recovered baseline code, not the validated Phase 1 contracts. Geometry, quantities, digests, resource estimates, provenance, and blob references still require the planned hardening.
- There is no CLI, evidence IR, engine API, process protocol, runtime, renderer, model store, OCR engine, foundry, or benchmark tool yet.
- The archived source import was truncated. Its complete original tree cannot be reconstructed from the committed fragments.
- The smoke test exercises only the offline core-library test path; end-to-end PDF smoke coverage begins in Phase 2.
- Branch protection and required checks are repository settings and must be enabled after the Phase 0 change is merged.
