#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

if ! git diff --quiet || ! git diff --cached --quiet || [[ -n $(git status --porcelain --untracked-files=all) ]]; then
  echo "release check requires a clean tracked and untracked worktree" >&2
  exit 1
fi

cargo metadata --locked --format-version 1 | jq -e '
  all(.packages[] | select(.source == null);
    .version == "0.2.0" and
    .license == "MIT OR Apache-2.0" and
    .repository == "https://github.com/akiselev/ferrodoc" and
    (.description | type == "string" and length > 0) and
    .publish == []
  )
' >/dev/null

scratch=$(mktemp -d)
trap 'rm -rf -- "$scratch"' EXIT
archive="$scratch/ferrodoc-v0.2.0.tar"
git archive --format=tar --prefix=ferrodoc-v0.2.0/ HEAD >"$archive"
tar -tf "$archive" >"$scratch/members.txt"

if rg -i '\.materialize|(^|/)(target|\.git)/|\.(rten|onnx|gguf|safetensors|dll|dylib|so)$|held.?out[^/]*truth|truth[^/]*held.?out' "$scratch/members.txt"; then
  echo "release archive contains a forbidden opaque, build, model, native, or held-out truth member" >&2
  exit 1
fi

tar -xf "$archive" -C "$scratch"
source_root="$scratch/ferrodoc-v0.2.0"
if rg -n --glob '!**/scripts/release-check.sh' '/home/[^/]+/|/Users/[^/]+/|C:\\\\Users\\\\|BEGIN (RSA |EC |OPENSSH )?PRIVATE KEY|AKIA[0-9A-Z]{16}' "$source_root"; then
  echo "release archive contains a local path or secret marker" >&2
  exit 1
fi
if find "$source_root/models" -type f \( -iname '*.rten' -o -iname '*.onnx' -o -iname '*.gguf' -o -iname '*.safetensors' \) -print -quit | grep -q .; then
  echo "release archive contains a model binary" >&2
  exit 1
fi

CARGO_NET_OFFLINE=true cargo install \
  --path "$source_root/crates/ferrodoc-cli" --locked --root "$scratch/install"
"$scratch/install/bin/ferrodoc" --version | grep -Fx 'ferrodoc 0.2.0'
"$scratch/install/bin/ferrodoc" convert \
  "$source_root/fixtures/pdf/born-digital.pdf" --output "$scratch/installed.md"
cmp "$source_root/fixtures/golden/born-digital.md" "$scratch/installed.md"

gzip -n -c "$archive" >"$scratch/ferrodoc-v0.2.0.tar.gz"
sha256sum "$scratch/ferrodoc-v0.2.0.tar.gz"
echo "release archive and clean offline install: ok"
