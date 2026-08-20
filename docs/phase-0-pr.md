# Phase 0: repository recovery and truthful baseline

## Goal and baseline

Recover or reject the unfinished source import and establish a coherent, reproducible workspace from `master` commit `1be7412de6b0b42d72401377151097f718cd1d36`.

## Summary

- Safely inspected the truncated materialization payload and recorded every recoverable member in `docs/recovery-inventory.md`.
- Removed the opaque payload, missing workspace members, targetless foundry manifest, and broken placeholder CLI, benchmark, and batteries crates.
- Retained only `ferrodoc-core`, the sole package with a complete target and dependency graph.
- Pinned Rust 1.95.0, committed `Cargo.lock`, and tested the stated MSRV.
- Added workspace-integrity and offline smoke scripts.
- Changed CI to check formatting, build with `--locked`, deny Clippy warnings, run the smoke test, and verify that validation leaves the checkout unchanged.
- Replaced unimplemented product claims and the abbreviated Apache notice with truthful documentation and the complete license text.

No package was renamed or migrated. Removed packages may be rebuilt under the reduced boundaries in later phases; no recovered engine source was copied into the workspace.

## Deviations

None. Optional native/model CI jobs are not created because Phase 0 contains no optional native or model package. They enter only when such a package is admitted by a later phase.

## Migration, risk, and rollback

There is no user-facing CLI to migrate. Downstream users of the broken package paths must wait for the Phase 1 and Phase 2 replacements. Rollback is the normal Git revert of commit `57e9c27`; the opaque payload also remains retrievable from commit `97b21fcaacc82bacc542e10487b235a505da4d53`.

The surviving core API is explicitly not stabilized by this phase. Phase 1 is expected to replace its invalid geometry, quantity, digest, blob, resource, and provenance contracts.

## Acceptance evidence

| Criterion | Evidence |
|---|---|
| `.materialize/` is absent | Removed and rejected by `scripts/check-workspace.sh`; `.gitignore` prevents accidental recommit. |
| Every workspace member exists and has a target | `scripts/check-workspace.sh` compares all local manifests with Cargo metadata and rejects empty target lists; passed. |
| Locked metadata resolves | `cargo metadata --locked --format-version 1` passed. |
| Pinned toolchain passes all Rust gates | Rust 1.95.0 passed format, check, test, and Clippy with warnings denied. |
| Claimed MSRV is tested or removed | `rustc 1.95.0 (59807616e 2026-04-14)` ran every local gate. |
| CI never modifies the checkout | CI uses format check mode and ends with `git diff --exit-code`; the same sequence passed locally. |
| Deterministic network-free smoke exists | `CARGO_NET_OFFLINE=true cargo test --locked -p ferrodoc-core --lib` passed. |
| README commands reference real targets | README lists only the seven Phase 0 validation commands, all executed successfully. Its three local links resolve. |
| Recovery inventory covers decoded paths | `docs/recovery-inventory.md` records fragment sizes/digests, safe extraction rules, all 29 complete members, classifications, and comparison with PR #1 claims. |
| Baseline is ready for required checks | The `core` workflow contains locked metadata, integrity, format, check, test, Clippy, smoke, and clean-tree steps. Enabling branch protection remains the post-merge repository-setting task. |

## Validation

All commands below passed locally on 2026-08-19 with Rust 1.95.0:

```text
cargo metadata --locked --format-version 1 > /dev/null
./scripts/check-workspace.sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
./scripts/smoke.sh
git diff --exit-code
```

GitHub-hosted CI was not executed locally because this branch has not been pushed. After merge, enable branch protection for `master` and require the `core` job.

## Follow-up

Phase 1 replaces the surviving baseline types with validated contracts and adds only the real package boundaries named by that phase. Phase 2 will add purpose-built PDF fixtures and exercise representative PDFs from `~/research/lightbulb` during development.
