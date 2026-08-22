# Bounded rule-based table engine

`table.rulebased` is an offline deterministic contract oracle for page-qualified table refinement.
It recognizes only consistent nonempty pipe-delimited rows in existing target-region text. It does
not rasterize PDFs, perform OCR, infer merged cells, or claim production datasheet-table quality.

Every output cell cites an exact UTF-8 source span. Geometry and geometry quality are inherited
unchanged from the source text evidence, so the engine cannot turn region- or page-level evidence
into fabricated cell boxes. The runtime records cited IDs as delta prerequisites and rejects spans
outside the exact `(page_id, region_id)` target.

The engine is qualified for shared engine conformance and embedded/process parity. The minimized
fixture isolates deterministic parsing and evidence resolution only; representative quality and
performance remain unmeasured. See [the FDX3 contract](../fdx/FDX3_TARGETED_TABLES.md).
