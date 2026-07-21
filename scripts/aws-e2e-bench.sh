#!/usr/bin/env bash
# AWS campaign entry for valence-e2e + valence-bench.
# Local WSL: use --dry-run only; full runs belong on the campaign host.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# Local/WSL defaults to 1; AWS campaign hosts typically leave this unset or raise it.
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"
export VALENCE_BENCH_HARDWARE="${VALENCE_BENCH_HARDWARE:-aws-unset}"
# Set VALENCE_BENCH_RELEASE=1 for capacity / published comparative numbers.
RELEASE_FLAG=()
if [[ "${VALENCE_BENCH_RELEASE:-}" == "1" || "${VALENCE_BENCH_RELEASE:-}" == "true" ]]; then
  RELEASE_FLAG=(--release)
fi

DRY_RUN=0
DO_E2E=0
BENCH_SLICE=""

usage() {
  cat <<'EOF'
Usage: aws-e2e-bench.sh [--dry-run] [--e2e] [--bench SLICE|all]

  --dry-run   Print expected tests/experiments and env presence; exit 0
  --e2e       Run cargo test -p valence-e2e -- --test-threads=1
  --bench S   Run matrix slice (read-hammer|query-real|hop-pairs|hop-chains|
              hybrid-compare|adapter-minimal|write-sweep|query-depth|overhead|all)
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run) DRY_RUN=1; shift ;;
    --e2e) DO_E2E=1; shift ;;
    --bench) BENCH_SLICE="${2:?}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown arg: $1" >&2; usage; exit 1 ;;
  esac
done

echo "== env presence =="
for v in DATABASE_URL VALENCE_MONGODB_URI VALENCE_REDIS_URL VALENCE_REDIS_URLS VALENCE_BENCH_ROCKSDB; do
  if [[ -n "${!v:-}" ]]; then
    echo "  $v=set"
  else
    echo "  $v=unset (wire/rocksdb rows will skip)"
  fi
done
echo "  VALENCE_BENCH_HARDWARE=$VALENCE_BENCH_HARDWARE"

echo "== expected e2e harnesses =="
echo "  matrix_catalog (all storages + catalog scenarios)"
echo "  model_runtime_catalog"
echo "  admin_runtime_catalog"
echo "  cross_backend_hops (depth-2 Cartesian + chain depths)"

echo "== expected bench slices =="
echo "  adapter-minimal write-sweep query-depth overhead"
echo "  read-hammer query-real hop-pairs hop-chains hybrid-compare"

if [[ "$DRY_RUN" -eq 1 ]]; then
  echo "dry-run complete"
  exit 0
fi

# Wire / rocksdb adapters need explicit crate features (env alone is not enough).
E2E_EXTRA_FEATURES=()
BENCH_EXTRA_FEATURES=()
if [[ -n "${DATABASE_URL:-}" ]]; then
  E2E_EXTRA_FEATURES+=("postgres" "hybrid")
  BENCH_EXTRA_FEATURES+=("postgres" "hybrid")
fi
if [[ "${VALENCE_BENCH_ROCKSDB:-}" == "1" ]]; then
  E2E_EXTRA_FEATURES+=("surreal-rocksdb")
  BENCH_EXTRA_FEATURES+=("surreal-rocksdb")
fi
features_args_from() {
  local -n _feats=$1
  if [[ ${#_feats[@]} -eq 0 ]]; then
    return 0
  fi
  local IFS=,
  echo --features "${_feats[*]}"
}
# shellcheck disable=SC2207
E2E_FEATURES_ARGS=($(features_args_from E2E_EXTRA_FEATURES))
# shellcheck disable=SC2207
BENCH_FEATURES_ARGS=($(features_args_from BENCH_EXTRA_FEATURES))

wipe_wire_stores() {
  echo "== wipe wire stores =="
  if [[ -n "${VALENCE_MONGODB_URI:-}" ]]; then
    if command -v mongosh >/dev/null 2>&1; then
      mongosh --quiet "$VALENCE_MONGODB_URI" --eval 'db.getSiblingDB("valence").dropDatabase()' >/dev/null 2>&1 || true
    else
      docker compose -f "$HOME/valence-services/docker-compose.yml" exec -T mongodb \
        mongosh --quiet --eval 'db.getSiblingDB("valence").dropDatabase()' >/dev/null 2>&1 || true
      sudo docker compose -f "$HOME/valence-services/docker-compose.yml" exec -T mongodb \
        mongosh --quiet --eval 'db.getSiblingDB("valence").dropDatabase()' >/dev/null 2>&1 || true
    fi
  fi
  if [[ -n "${DATABASE_URL:-}" ]]; then
    if command -v psql >/dev/null 2>&1; then
      psql "$DATABASE_URL" -c 'DROP SCHEMA public CASCADE; CREATE SCHEMA public;' >/dev/null 2>&1 || true
    else
      docker compose -f "$HOME/valence-services/docker-compose.yml" exec -T postgres \
        psql -U valence -d valence -c 'DROP SCHEMA public CASCADE; CREATE SCHEMA public;' >/dev/null 2>&1 || true
      sudo docker compose -f "$HOME/valence-services/docker-compose.yml" exec -T postgres \
        psql -U valence -d valence -c 'DROP SCHEMA public CASCADE; CREATE SCHEMA public;' >/dev/null 2>&1 || true
    fi
  fi
  if [[ -n "${VALENCE_REDIS_URL:-}${VALENCE_REDIS_URLS:-}" ]]; then
    if command -v redis-cli >/dev/null 2>&1; then
      redis-cli FLUSHALL >/dev/null 2>&1 || true
    else
      docker compose -f "$HOME/valence-services/docker-compose.yml" exec -T redis redis-cli FLUSHALL >/dev/null 2>&1 || true
      sudo docker compose -f "$HOME/valence-services/docker-compose.yml" exec -T redis redis-cli FLUSHALL >/dev/null 2>&1 || true
    fi
  fi
}

if [[ "$DO_E2E" -eq 1 ]]; then
  export CARGO_TARGET_DIR=target-valence-e2e
  echo "== e2e =="
  # Run harnesses separately so one failure does not skip the rest of the campaign.
  e2e_rc=0
  for harness in admin_runtime_catalog model_runtime_catalog matrix_catalog cross_backend_hops; do
    wipe_wire_stores
    echo "== e2e harness ${harness} =="
    if ! cargo test -p valence-e2e "${E2E_FEATURES_ARGS[@]}" --test "$harness" -- --test-threads=1; then
      echo "E2E_HARNESS_FAIL:${harness}"
      e2e_rc=1
    else
      echo "E2E_HARNESS_OK:${harness}"
    fi
  done
  if [[ "$e2e_rc" -ne 0 ]]; then
    echo "E2E_EXIT:1 (one or more harnesses failed)"
    exit 1
  fi
  echo "E2E_EXIT:0"
fi

run_slice() {
  local slice="$1"
  wipe_wire_stores
  export CARGO_TARGET_DIR=target-valence-bench
  local storages="mem,sqlite,surreal-mem,indradb"
  if [[ -n "${DATABASE_URL:-}" ]]; then
    storages+=",postgres,hybrid"
  fi
  if [[ -n "${VALENCE_MONGODB_URI:-}" ]]; then
    storages+=",mongodb"
  fi
  if [[ -n "${VALENCE_REDIS_URL:-}${VALENCE_REDIS_URLS:-}" ]]; then
    storages+=",redis"
  fi
  if [[ "${VALENCE_BENCH_ROCKSDB:-}" == "1" ]]; then
    storages+=",surreal-rocksdb"
  fi
  # Focused list for bm-v26 (must be last so mongo/redis are not appended).
  if [[ "$slice" == "hybrid-compare" ]]; then
    if [[ -n "${DATABASE_URL:-}" ]]; then
      storages="hybrid,postgres,indradb"
    else
      storages="indradb"
    fi
  fi
  echo "== bench matrix $slice storages=$storages =="
  cargo run -p valence-bench "${RELEASE_FLAG[@]}" "${BENCH_FEATURES_ARGS[@]}" -- matrix "$slice" --storage "$storages"
}

if [[ -n "$BENCH_SLICE" ]]; then
  if [[ "$BENCH_SLICE" == "all" ]]; then
    for s in adapter-minimal write-sweep query-depth overhead read-hammer query-real hop-pairs hop-chains hybrid-compare; do
      run_slice "$s"
    done
  else
    run_slice "$BENCH_SLICE"
  fi
fi

if [[ "$DO_E2E" -eq 0 && -z "$BENCH_SLICE" ]]; then
  usage
  exit 1
fi
