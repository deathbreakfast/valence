#!/usr/bin/env bash
# Valence upstream release gates — run before every commit/tag.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

fail() {
  echo "gate FAILED: $1" >&2
  exit 1
}

COMMON_EXCLUDES=(
  --glob '!.git/**'
  --glob '!target/**'
  --glob '!target-*/**'
)

echo "== gate: family / product crate names =="
if rg -n -i 'prioritization\.md|leptos|valence-wiring|valence-platform|valence-spectra|chronon-valence|boson-valence|\bgluon\b|gluon_registry|valence-secrets-gauge|secrets-gauge|\bhiggs\b|\bsoliton\b|nucleus-core|\bnucleus\b' . \
  "${COMMON_EXCLUDES[@]}" \
  --glob '!scripts/gate.sh' \
  --glob '!README.md' \
  --glob '!valence-backend-surreal/README.md' 2>/dev/null; then
  fail "banned host/product reference in source or manifests"
fi

echo "== gate: host product table / schema prefixes in Rust =="
if rg -n 'chronon_|boson_|photon_|gluon_registry|_spectra_' --glob '*.rs' . \
  "${COMMON_EXCLUDES[@]}" \
  --glob '!scripts/gate.sh' 2>/dev/null; then
  fail "host product table/schema prefix found in Rust sources"
fi

echo "== gate: topology words in docs =="
if rg -n -i '\bmonolith(ic)?\b' . \
  --glob '*.md' --glob '*.rs' \
  "${COMMON_EXCLUDES[@]}" \
  --glob '!README.md' \
  --glob '!scripts/gate.sh' 2>/dev/null; then
  fail "monolith vocabulary in docs/sources"
fi

echo "== gate: ValenceMode / closed mode enums =="
if rg -n 'ValenceMode|VALENCE_MODE' . --glob '*.rs' \
  "${COMMON_EXCLUDES[@]}" 2>/dev/null; then
  fail "ValenceMode found in Rust sources"
fi

echo "== gate: spectra / surrealdb in valence-core =="
if rg -n 'spectra' valence-core/Cargo.toml valence-core/src/ 2>/dev/null; then
  fail "spectra leaked into valence-core"
fi
if rg -n 'surrealdb' valence-core/Cargo.toml 2>/dev/null; then
  fail "surrealdb dependency in valence-core manifest"
fi
if rg -n 'use surrealdb|surrealdb::' valence-core/src/ 2>/dev/null; then
  fail "surrealdb crate usage in valence-core sources"
fi

echo "== gate: nucleus in workspace manifests =="
if rg -n 'nucleus' --glob 'Cargo.toml' . \
  "${COMMON_EXCLUDES[@]}" 2>/dev/null; then
  fail "nucleus dependency in Cargo.toml"
fi

echo "all gates passed"
