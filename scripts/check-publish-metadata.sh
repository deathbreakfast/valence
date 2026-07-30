#!/usr/bin/env bash
# Verify publishable uf-* crates have the metadata required for crates.io.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

required_keys=(repository homepage documentation keywords categories)
publishables=(
  valence/Cargo.toml
  valence-core/Cargo.toml
  valence-schema-dsl/Cargo.toml
  valence-macros/Cargo.toml
  valence-telemetry/Cargo.toml
  valence-codegen/Cargo.toml
  valence-backend-mem/Cargo.toml
  valence-backend-sql/Cargo.toml
  valence-backend-sqlite/Cargo.toml
  valence-backend-postgres/Cargo.toml
  valence-backend-mongodb/Cargo.toml
  valence-backend-indradb/Cargo.toml
  valence-backend-hybrid/Cargo.toml
  valence-backend-redis/Cargo.toml
  valence-backend-surreal/Cargo.toml
)

fail=0
for manifest in "${publishables[@]}"; do
  name=$(grep -m1 '^name = ' "$manifest" | cut -d'"' -f2)
  if [[ "$name" != uf-* ]]; then
    echo "error: $manifest package name is '$name' (expected uf-*)"
    fail=1
  fi
  if ! grep -q 'repository.workspace = true' "$manifest"; then
    echo "error: $manifest missing repository.workspace = true"
    fail=1
  fi
  if ! grep -q 'homepage.workspace = true' "$manifest"; then
    echo "error: $manifest missing homepage.workspace = true"
    fail=1
  fi
  for key in documentation keywords categories; do
    if ! grep -q "^${key} = " "$manifest"; then
      echo "error: $manifest missing $key"
      fail=1
    fi
  done
  if grep -q '^\[lib\]' "$manifest"; then
    lib_name=$(awk '/^\[lib\]/{f=1;next} f&&/^name = /{gsub(/"/,"",$3); print $3; exit}' "$manifest")
    if [[ -z "$lib_name" ]]; then
      echo "error: $manifest [lib] missing name (needed to preserve Rust import path)"
      fail=1
    fi
  fi
done

if grep -q 'publish = false' valence-testkit/Cargo.toml; then
  :
else
  echo "error: valence-testkit must set publish = false"
  fail=1
fi

if [[ "$fail" -ne 0 ]]; then
  exit 1
fi

echo "publish metadata ok (${#publishables[@]} crates + testkit non-publishable)"
