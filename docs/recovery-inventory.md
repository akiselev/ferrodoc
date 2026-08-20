# Source recovery inventory

## Outcome

The `.materialize/` payload committed by PR #1 was copied to a read-only temporary directory before inspection. It is a base64-encoded gzip-compressed tar stream, but it is truncated: 35,777 encoded bytes yield 26,832 compressed bytes and 134,058 inflated bytes, and the gzip end-of-stream marker is absent. The complete original tree therefore cannot be reconstructed from the repository.

The recoverable tar prefix contained 29 complete members (16 regular files). Before extraction, every member was checked for absolute paths, parent traversal, device nodes, links, and resolution outside the temporary root. None violated those checks. Only validated directories and regular files were written to the temporary recovery directory.

No recovered source was admitted to the workspace. The profiles use an absent schema and are retained only as design reference in Git history. The engine files depend on absent APIs, represent engines deferred beyond v0.2, and do not collectively form complete crates, so they are discarded from the working tree.

## Encoded fragments

Fragments were concatenated in numeric order implied by their names: `part00`, `pair01_02`, `pair03_04`, `pair05_06`.

| Fragment | Bytes | SHA-256 |
|---|---:|---|
| `.materialize/part00` | 6,000 | `536ed1538cc2c7be663df6cc43c0865a300afd4c389fd2ae92e620bb3d152d28` |
| `.materialize/pair01_02` | 11,999 | `1e64368f83ca32f2910c1cd3ec0ec3d662181c1ca2ac4f400389e64f8ceef6ab` |
| `.materialize/pair03_04` | 5,777 | `37c9357bb6c0b89532b4404c89897086a3990ea7993be35a33ae5043190e5dac` |
| `.materialize/pair05_06` | 12,001 | `f0ff79ea8ed4b04be2749c0e123d59ca09db4511543031dce4a6ccebdd9d0548` |

The fragment lengths are arbitrary cuts through one base64 stream. Decoding the largest complete base64 prefix produces a valid streaming gzip prefix, not four independent objects.

## Recovered members

Every complete tar member is listed below. `salvage-reference` means the item may inform a later clean implementation but is not copied into the repository. `discard` means it has no role in the Phase 0 tree.

| Path | Kind | Classification | Bytes | SHA-256 or rationale |
|---|---|---|---:|---|
| `ferrodoc-work/` | directory | discard | 0 | archive container |
| `ferrodoc-work/profiles/` | directory | salvage-reference | 0 | planned policy inputs; schema absent |
| `ferrodoc-work/profiles/low-vram.toml` | file | salvage-reference | 237 | `45fefe4d2215dc141460cc49bd3da4a514dd5d61547a52fe818d380504c4a38c` |
| `ferrodoc-work/profiles/cpu.toml` | file | salvage-reference | 180 | `e451f1627269c39df67019e65e83a920f11dcaea61747d1595fc115fc85b7493` |
| `ferrodoc-work/profiles/accurate.toml` | file | salvage-reference | 168 | `d14b1f6f12fe37de36e47573288131f8a7c3344ee86392bf2786065b75f98257` |
| `ferrodoc-work/plugins/` | directory | discard | 0 | superseded package layout |
| `ferrodoc-work/plugins/ferrodoc-engine-llamacpp/` | directory | discard | 0 | engine deferred beyond v0.2 |
| `ferrodoc-work/plugins/ferrodoc-engine-llamacpp/src/` | directory | discard | 0 | engine deferred beyond v0.2 |
| `ferrodoc-work/plugins/ferrodoc-engine-llamacpp/src/lib.rs` | file | discard | 13,047 | `91ef0beca056caa41546dd2cd582a34583c360316bdc525f87eaef462def9fa4` |
| `ferrodoc-work/plugins/ferrodoc-engine-llamacpp/src/main.rs` | file | discard | 172 | `af2fb4344de96c7538fa37b8f2d9d2915d58b5b5401b9c3f7c2cca6877431568` |
| `ferrodoc-work/plugins/ferrodoc-engine-llamacpp/Cargo.toml` | file | discard | 657 | `24feca3ff99f33b7d58f3f602aa9613fbca3f9ea78be32151a7ab2c1b9d8e11d` |
| `ferrodoc-work/plugins/ferrodoc-engine-oar-classic/` | directory | discard | 0 | engine deferred beyond v0.2 |
| `ferrodoc-work/plugins/ferrodoc-engine-oar-classic/src/` | directory | discard | 0 | engine deferred beyond v0.2 |
| `ferrodoc-work/plugins/ferrodoc-engine-oar-classic/src/lib.rs` | file | discard | 9,665 | `1106563986971b3df6abec334ce73809dd89707223e81ca31bfee1f64683dec3` |
| `ferrodoc-work/plugins/ferrodoc-engine-oar-classic/src/main.rs` | file | discard | 127 | `4491b5507f8eaea7b719649b379b63c070e3d5596b940dab925364566ffabc6b` |
| `ferrodoc-work/plugins/ferrodoc-engine-oar-classic/Cargo.toml` | file | discard | 640 | `89f31cd2c6807a41b7f1aceac0569a9289b6cbc3a059223cecdb19f4bef1cd7d` |
| `ferrodoc-work/plugins/ferrodoc-engine-remote/` | directory | discard | 0 | hosted engines deferred beyond v0.2 |
| `ferrodoc-work/plugins/ferrodoc-engine-remote/src/` | directory | discard | 0 | hosted engines deferred beyond v0.2 |
| `ferrodoc-work/plugins/ferrodoc-engine-remote/src/lib.rs` | file | discard | 3,091 | `1adfc9095a6b7b6e4ccb719109870e90d6e78ba2baf98acc0eddf865f3638472` |
| `ferrodoc-work/plugins/ferrodoc-engine-remote/src/main.rs` | file | discard | 166 | `244f560e8abbb279b49ecad6f931063c2e1796e5cd78663f91709d21ac09848d` |
| `ferrodoc-work/plugins/ferrodoc-engine-remote/Cargo.toml` | file | discard | 540 | `551363c6da237f00f9e9f2d436ed252d4738b23823021ab6460e57fed5c0ef59` |
| `ferrodoc-work/plugins/ferrodoc-engine-mistralrs/` | directory | discard | 0 | engine deferred beyond v0.2 |
| `ferrodoc-work/plugins/ferrodoc-engine-mistralrs/src/` | directory | discard | 0 | engine deferred beyond v0.2 |
| `ferrodoc-work/plugins/ferrodoc-engine-mistralrs/src/lib.rs` | file | discard | 8,046 | `9f72773e6c37133a7ff4bb554aa635d9acdfef4e215ec5166cce6c8421daab4b` |
| `ferrodoc-work/plugins/ferrodoc-engine-mistralrs/src/main.rs` | file | discard | 175 | `ea290d8d4bf3358676012e657038d257dccb4480ca8418627836608c97b121b3` |
| `ferrodoc-work/plugins/ferrodoc-engine-mistralrs/Cargo.toml` | file | discard | 597 | `92c04f56f8936ec00d93229ef2cc7a36e14d51914d17227e0d89c8bdf8a8be10` |
| `ferrodoc-work/plugins/ferrodoc-engine-oar/` | directory | discard | 0 | engine deferred beyond v0.2 and crate truncated |
| `ferrodoc-work/plugins/ferrodoc-engine-oar/src/` | directory | discard | 0 | engine deferred beyond v0.2 and crate truncated |
| `ferrodoc-work/plugins/ferrodoc-engine-oar/src/lib.rs` | file | discard | 13,066 | `e457bf6bcfc0bd65728fc17bf741f926becd051997e886f87a115b4e2983308a` |

## Comparison with repository claims

- PR #1 added the four payload fragments after committing a broad manifest and README, but never committed the decoded tree as normal source.
- The recovered engine names match four of the old manifest and README entries. Their manifests refer to missing host/API crates, and the OAR crate is incomplete because the archive ends after its `lib.rs`.
- None of the recovered material supplies the 26 missing workspace packages, a complete CLI dependency graph, `xtask`, scripts, models, architecture documents, or a buildable foundry target.
- The three recovered profiles support the claim that CPU, accurate, and low-VRAM policies were contemplated, but they do not establish an implemented or validated planner.
- The committed `ferrodoc-core` source remains the only defensible package at the Phase 0 boundary. The broken CLI, benchmark, foundry, and batteries placeholders were removed rather than represented as working packages.

The original fragments remain recoverable from Git commit `97b21fcaacc82bacc542e10487b235a505da4d53`; `.materialize/` is ignored so local forensic work cannot accidentally be recommitted.
