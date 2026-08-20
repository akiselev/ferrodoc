# Third-party notices

Ferrodoc source is dual-licensed under MIT or Apache-2.0. Its Rust dependency graph is resolved exactly by `Cargo.lock` and checked by `cargo deny check` against `deny.toml`.

Accepted dependency licenses are Apache-2.0, Apache-2.0 with LLVM exception, BSD-2-Clause, BSD-3-Clause, ISC, MIT, Unicode-3.0, Unlicense, and Zlib. An expression with several alternatives is accepted only when at least one allowed alternative applies. The authoritative crate names, versions, sources, and license expressions are the locked Cargo metadata; this summary does not replace upstream notices.

Optional integrations have separate obligations:

- OCRS Rust code follows its crate metadata. The separately acquired pretrained model pair is CC-BY-SA-4.0; its manifest, source, attribution notice, exact digests, and acceptance prompt are in `models/ocrs-cpu.json`.
- Tesseract and Leptonica are system-provided optional libraries. Ferrodoc does not redistribute them. Their licenses and language-data terms must be reviewed for the installed platform packages.
- The experimental command engine executes an administrator-selected binary that Ferrodoc does not redistribute or license.

No model binary or native runtime is included in the v0.2 source archive.
