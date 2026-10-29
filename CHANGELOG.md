# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `Actor::is_service_user()` and `Actor::service_name()`. `RejectExternalSystemActor` is
  renamed to `RejectExternalPrivilegedActor` (old name kept as a deprecated alias).
  Behavior change: the `AUTHENTICATED` privacy rule now allows `Actor::ServiceUser`
  as well as `Actor::User` and `Actor::System` — a service user is authenticated by
  definition; exclude it by writing a custom rule whose check is `viewer.is_user()`
  on its own instead of reusing `AUTHENTICATED`.

### Fixed

- The document-row filter shared by the mem, IndraDB, Redis, MongoDB and hybrid backends
  flattened `(a = $x) AND (b = $y)` into one `OR`. A query filtering two record fields at
  once (a team and a user, say) returned every row matching either field, so a lookup for a
  user who is not on a team could return another member's edge. Every `AND` group must now
  match, and a term the filter cannot read excludes the row.

### Added

- `uf-valence-data-use-scan` purpose lint bans six more migration template sentences
  under `BAN_TEMPLATE` and adds `HEDGE` for "not necessarily" and "unless that feature".
  New `valence-purpose-lint` binary, `lint_tree` and `HitSummary` count hits per file.
  `GapCode` is now `#[non_exhaustive]`; matches on it need a wildcard arm.
  Behavior change: purposes that carry any of the eight template phrases or the two
  hedges now fail `lint_purpose` and `lint_scan_hits`, so a downstream test or CI gate
  on this crate goes red until those `use_!` purposes are rewritten from the code.

- Purpose quality lint in `uf-valence-data-use-scan` (`lint_purpose`, tier-aware gap
  codes, inventory helpers) so migration template purposes cannot re-land unnoticed.
  Fleet call-site `use_!` strings rewritten toward stranger-readable trust narratives
  (bold key terms; honest display vs internal use).

- Declared data-use transparency: Model / trait / `QueryCore` entry points require
  `use_!(…)` purpose markdown (file/line captured for the catalog). There is no
  purpose-free public overload. Schema/trait `repository:` is required for View source
  links. `uf-valence-data-use-scan` builds the snapshot consumed by valence-uf-app Data
  uses / Unscoped uses UI.

### Breaking

- Model / trait / `QueryCore` / connection / Unscoped free-function entry points take a
  required `DataUsePurpose` (`use_!(…)`) under the primary method names (`get`, `create`,
  `query`, …). Purpose-free public overloads are removed.

### Fixed

- SQLite in-memory backends (`:memory:`, `mode=memory`) no longer lose their data when
  the pool replaces its connection after a cancelled query, idle timeout, or max
  lifetime. The backend holds one extra connection open so SQLite keeps the database,
  and each bare `:memory:` backend gets its own uniquely named database.
- SQL / mem filters for Currency subfields (`where_{field}_code` /
  `where_{field}_minor`): dotted paths emit `json_extract` (Postgres rewrite +
  integer cast for `amount_minor`); mem row-filter parses typed-column extracts
  and AND-conjunct LHS segments.
- Catalog scenarios `query-filter-currency` / `query-filter-currency-miss`.

### Added

- Crate-root Features + guides for **Currency fields** and **DateTime unix
  storage** (typed money cell + UTC unix seconds persistence).

### Breaking

- SQL adapters store schema fields as real columns (SQLite/Postgres). The old
  `(id, body)` JSON document layout is gone. Recreate SQLite files / wipe wire
  Postgres schemas after upgrade (`DROP SCHEMA public CASCADE` on campaign hosts).
- Redis records use Hash fields per column (not one JSON STRING). Flush Redis
  before re-running campaigns.
- IndraDB stores one vertex property per field (not a single `body` property).
- Surreal schema-backed tables use `SCHEMAFULL` + `DEFINE FIELD` via
  `ensure_typed_table` / `sync_typed_table`.
- Schema layout alters for registered models are **boot-gated** by DSL
  `Schema.version` vs `valence_schema_meta` stamps. Hosts must call
  `Valence::sync_typed_tables_from_registry` at startup. Bump `version` when
  fields change; steady-state writes no longer run catalog inspect every time.

### Added

- `valence_core::storage_layout` — `StorageLayout` from schema metadata, dialect
  DDL export (`to_ddl`), inspect / ensure / additive sync on `DatabaseBackend`.
- `Valence::ensure_typed_tables_from_registry` /
  `sync_typed_tables_from_registry` (version-gated skip when stamp matches).
- `SafeTweak` layout ops (Postgres nullability / DEFAULT); SQLite nullability
  change refused.
- `valence_schema_meta` stamp table + `read_schema_version` /
  `write_schema_version` on SQL backends.
- Generated `{Model}Schema::storage_layout()` beside `full()` / `metadata()`.

## [0.1.1] - 2026-07-19

This release prepares Valence for its first public publication under:

https://github.com/unified-field-dev/valence

### Highlights

- Publishable crates renamed to the free crates.io namespace `uf-*` (for example `uf-valence`, `uf-valence-core`) while keeping Rust import paths (`valence`, `valence_core`, …)
- Normalized repository metadata across workspace crates
- Corrected public documentation links and ownership references
- Added public community-health files and security policy
- Hardened GitHub Actions workflows (pinned actions, least-privilege permissions, package dry-run)
- Dependabot and CodeQL deferred until the repository is public / more mature
- Clarified publication intent for internal-only test and bench crates (`valence-testkit` is not published)
- Tightened hop-query test skips into an explicit capability matrix

### Notes

- `valence-testkit`, `valence-e2e`, and `valence-bench` are internal support crates and are not part of the public crates.io surface
- Wire-backend examples still require live service URLs
- Cross-backend transaction semantics remain unsupported
- Some advanced hop-query behavior remains pre-1.0; unsupported combinations are skipped explicitly rather than soft-passing

### Migration

Replace Cargo dependency names when consuming from git or crates.io:

```toml
uf-valence = { git = "https://github.com/unified-field-dev/valence", package = "uf-valence", features = ["mem"] }
```

Rust code continues to use `use valence::…` (crate `[lib] name` is unchanged).
