#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"
scratch=$(mktemp -d)
trap 'rm -rf -- "$scratch"' EXIT

manifest=models/ocrs-cpu.json
source=$(jq -r '.license.source' "$manifest")
jq -c '.files[]' "$manifest" | while read -r file; do
  path=$(jq -r '.path' <<<"$file")
  digest=$(jq -r '.digest' <<<"$file")
  bytes=$(jq -r '.bytes' <<<"$file")
  curl --fail --location --silent --show-error "$source$path" --output "$scratch/$path"
  test "$(wc -c <"$scratch/$path")" -eq "$bytes"
  printf '%s  %s\n' "$digest" "$scratch/$path" | sha256sum --check
done

echo "model manifest links and digests: ok"
