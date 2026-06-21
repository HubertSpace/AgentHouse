#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

tree_file="$(mktemp)"
trap 'rm -f "$tree_file"' EXIT

cargo tree -p agenthouse --edges normal,build --format '{p}\t{l}' >"$tree_file"

if grep -E '(^|[[:space:]])(zlog|ztracing|ztracing_macro) v' "$tree_file"; then
  echo "error: GPL tracing crates are present in the agenthouse dependency graph" >&2
  exit 1
fi

if grep -E '(^|[^A-Z])(AGPL|GPL|LGPL)-' "$tree_file"; then
  echo "error: copyleft GPL-family license found in the agenthouse dependency graph" >&2
  exit 1
fi

if grep -E '(^|[^A-Z])MPL-' "$tree_file"; then
  echo "error: MPL license found in the agenthouse dependency graph" >&2
  exit 1
fi

echo "release license check passed"
