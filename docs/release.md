# v0.2 release procedure

Ferrodoc v0.2 is distributed as a source archive from a protected Git tag. Workspace crates are intentionally marked `publish = false`; v0.2 is not a crates.io publication. The supported installation command is `cargo install --path crates/ferrodoc-cli --locked` from the source tree or release archive.

Run the locked validation gate and `./scripts/release-check.sh`. The release check builds a Git archive from the exact commit, scans its member list and text for forbidden opaque payloads, local paths, common secret markers, held-out truth artifacts, and model/native binaries, then installs and exercises the CLI from the extracted archive with Cargo offline.

The workspace stays at its development version (`0.1.0`) until the release
operation. Bumping `[workspace.package] version` to the release number is part
of the release itself: `scripts/release-check.sh` derives the expected archive,
CLI banner, and metadata version from the workspace manifest, so the bump must
land before the check runs.

Before tagging, verify all required GitHub checks are green and configure branch protection on `master` using `.github/required-checks.json`. Then create annotated tag `v$VERSION` at that exact green merge commit and attach release notes from `RELEASE_NOTES.md`. Never move or recreate the tag.

The tag is intentionally forbidden while branch protection or any release
criterion is unverified. Repository settings and remote tag publication are
maintainer operations; local validation cannot truthfully substitute for them.
