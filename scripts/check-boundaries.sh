#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

for package in ferrodoc-core ferrodoc-ir ferrodoc-engine-api ferrodoc-protocol; do
  dependencies=$(cargo tree --locked --edges normal --prefix none -p "$package")
  if grep -Eq '^(tokio|reqwest|hyper|candle|ort|ocrs|rten|lopdf|hayro|tesseract|llama-cpp|mistralrs|cuda)[[:space:]]+v' <<<"$dependencies"; then
    echo "error: forbidden runtime dependency reached $package" >&2
    echo "$dependencies" >&2
    exit 1
  fi
done

echo "runtime-agnostic dependency boundaries: ok"
