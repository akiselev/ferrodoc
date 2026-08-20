#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

if [[ -e .materialize ]]; then
  echo "error: opaque .materialize payload is forbidden" >&2
  exit 1
fi

metadata=$(cargo metadata --locked --no-deps --format-version 1)
if grep -q '"targets":\[\]' <<<"$metadata"; then
  echo "error: cargo metadata contains a package with no targets" >&2
  exit 1
fi

declared_file=$(mktemp)
metadata_file=$(mktemp)
trap 'rm -f -- "$declared_file" "$metadata_file"' EXIT

for package_root in crates engines tools; do
  if [[ -d $package_root ]]; then
    find "$package_root" -name Cargo.toml -not -path '*/target/*' -print
  fi
done | sort >"$declared_file"

grep -o '"manifest_path":"[^"]*"' <<<"$metadata" \
  | sed -e 's/^"manifest_path":"//' -e 's/"$//' -e "s#^$repo_root/##" \
  | sort >"$metadata_file"

if ! diff -u "$declared_file" "$metadata_file"; then
  echo "error: local Cargo manifests and workspace packages differ" >&2
  exit 1
fi

echo "workspace integrity: ok"
