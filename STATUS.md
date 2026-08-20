# Status

- Phase 0 is in progress from `1be7412e440fa1ab72c32d84150ffbe5048e5314`.
- Baseline reproduced: Cargo metadata fails on the absent `ferrodoc-ir` member.
- The `.materialize` payload was copied out of tree and identified as a truncated base64-encoded gzip/tar stream; safe recovery found 29 complete members for inventory review.
- Next: remove the opaque payload and broken placeholders, make workspace membership exact, then repair CI and documentation.
