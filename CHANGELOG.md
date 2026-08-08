# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
uf-valence = { git = "https://github.com/deathbreakfast/valence", package = "uf-valence", features = ["mem"] }
```

Rust code continues to use `use valence::…` (crate `[lib] name` is unchanged).
