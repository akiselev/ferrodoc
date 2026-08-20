# Known limitations

- The CLI reports version and foundation status only. It cannot inspect or convert PDFs.
- The PDF crate defines acquisition identity and parser limits but intentionally has no parser or rasterizer until Phase 2.
- The render crate emits canonical evidence JSON only; Markdown and HTML return explicit unsupported errors until Phase 2.
- Process-protocol message types exist, but framing, child lifecycle, cancellation acknowledgement, and blob resolution are not implemented until Phase 3.
- The runtime supports explicit embedded-engine registration only. It has no planner, scheduler, cache, model store, hardware probe, or process discovery.
- There is no OCR/layout engine, foundry, benchmark tool, or research loop yet.
- The archived source import was truncated. Its complete original tree cannot be reconstructed from the committed fragments.
- The smoke test exercises the offline workspace and truthful CLI status; end-to-end PDF smoke coverage begins in Phase 2.
- Branch protection and required checks are repository settings and must be enabled after the Phase 0 change is merged.
