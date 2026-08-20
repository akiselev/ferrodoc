# Status

- Phase 0 is implemented on `phase-0/recover-repository` from baseline `1be7412de6b0b42d72401377151097f718cd1d36`.
- The truncated source payload is inventoried and removed; the workspace now contains only the compiling `ferrodoc-core` package.
- Rust 1.95.0 passes locked metadata, integrity, format, check, test, Clippy, and the offline smoke test.
- Remaining Phase 0 operations: merge the phase change, enable the `core` check as required branch protection, then mark the ledger complete.
