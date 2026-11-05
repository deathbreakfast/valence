# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Documented

- The owner rules (`OWNER_BY_ID`, `OWNER_BY_USER_FIELD`, `check_owner_by_field`) never
  match an `Actor::ServiceUser`, since a service user has no user id. Allow a bot with a
  custom rule on `service_name()`; `SYSTEM_ONLY` never admits a service user, and
  `AUTHENTICATED` would also open the records to every signed-in user.

### Fixed

- The in-memory row filter's `ORDER BY` (used by the mem, redis, mongodb, indradb and hybrid
  backends) now sorts on every key with its own ASC/DESC, compares numbers numerically
  instead of skipping them, and reads `START n` as an offset.

### Changed

- The boot notice for schema TTL on Deferred backends (SQLite, Postgres, in-memory, Surreal)
  is now logged at `info` instead of `warn`. These backends already get the expire-at index;
  the notice only reminds the host to register `valence_platform::ttl_sweep`. Backends with
  no TTL path still warn. The once-per-table emit counter and metric are unchanged.

### Added

- `ValenceBuilder::extension` and `Valence::extension` give each `Valence` a typed slot for
  host ports that privacy evaluators consult. A host can keep something like a bot gate on
  the handle instead of in a process-global, and two runtimes in one process no longer share
  it. `with_actor` and `with_owner_override` keep the slot.
- `edge_policy: SourceUpdateTargetRead` on `ManyToMany` connections in `valence_schema!`.
  On an edge that sets it, `Valence::relate_edge` and `unrelate_edge` (and the generated
  `relate_to_*` / `unrelate_from_*`) load both rows and refuse the write unless the actor
  passes the source schema's Update policy and the target schema's Read policy: a missing
  row is `Error::NotFound`, a denial is `Error::Privacy`, and nothing is written. Edges
  without it still write raw, and deletion's edge cleanup stays unchecked. The attribute on
  any other cardinality is a compile error. `SchemaConnection` gains `edge_policy`, so code
  that builds it with a struct literal must add the field (`None`).
- `spectra_export: { cadence, job_name?, target_table? }` in `valence_schema!`. The block opts a
  table in to periodic Spectra export. `cadence` is `Hourly`, `ThreeHours`, `SixHours`,
  `TwelveHours`, `Daily` or `Weekly`; cron strings are rejected because the schedule is
  platform-owned (`PartitionExportCadence::cron`). `job_name` defaults to
  `{table}_{cadence}_export` and `target_table` to `{table}_export`. The resolved config is
  `Schema::partition_export`; the core name is neutral because `valence-core` keeps Spectra
  out of its source. Code that builds a `Schema` with a struct literal must add the field
  (`None`). Valence only records the opt-in; Chronon registration and the export executor
  live in other crates.
- `retired_fields: ["old"]` in `valence_schema!` (and `Schema::retired_fields`). A product
  can now remove a field whose column still exists: SQLite and Postgres boot sync keeps the
  column instead of refusing to start, typed reads leave it out, and new rows leave it NULL.
  A `NOT NULL` retired column without a default is relaxed on Postgres and refused on
  SQLite. Columns that are missing from the schema and not listed still fail sync as
  before. `Schema` gains `retired_fields` and `StorageLayout` gains `retired`, so code that
  builds either with a struct literal must add the field (`Vec::new()`).
- `QueryCore::execute_with_stats` and generated `fetch_with_stats` return a `QueryStats`
  whose `fetched` is the number of rows the backend returned for the window, before read
  privacy. Page until `fetched < limit` to tell the end of a table from a window privacy
  hid. It is not for display, and is an upper bound for trait-union queries.
- `DatabaseBackend::scan_table` and `scan_edges`: keyset-paged enumeration of a table's
  rows and of all edges, implemented for SQLite and Postgres (other engines return an
  unsupported error by default). `EdgeRow` is the edge row and cursor type.
- `Actor::is_service_user()` and `Actor::service_name()`. `RejectExternalSystemActor` is
  renamed to `RejectExternalPrivilegedActor` (old name kept as a deprecated alias).
  Behavior change: the `AUTHENTICATED` privacy rule now allows `Actor::ServiceUser`
  as well as `Actor::User` and `Actor::System` — a service user is authenticated by
  definition; exclude it by writing a custom rule whose check is `viewer.is_user()`
  on its own instead of reusing `AUTHENTICATED`.

### Changed

- The `valence_error_log` and `valence_slow_op` event payloads now carry the table name as
  `table_name` instead of `table`. Spectra's schema macro generates its own `table` field,
  which kept these events from being registered. Readers of the old key must switch.

### Fixed

- `valence_codegen::build()` and `generate_models` now return an error when a schema or
  trait file fails codegen, naming the file. They used to print a `cargo:warning` (which
  Cargo hides for git dependencies) and skip the model, so consumers only saw an
  unresolved import.
- A rule in a field `read` policy that is not a sync `PrivacyRule` (for example an async
  `PolicyEvaluator`) now fails the build. Codegen emits a per-rule assertion named after the
  table, field and bucket; before, such a schema compiled and every read of the field failed.
- Generated `relate_to_*` / `unrelate_from_*` (and the `_record` trait-target variants) wrote
  edges with no privacy check, so any code holding a user or bot `Valence` could add or drop
  a membership or grant edge. They now run the stored source record's update privacy first (not the caller's copy).
  Behavior change: a non-System actor that fails the source schema's update policy (or whose
  schema has none) gets `Error::Privacy`. `Valence::relate_edge` / `unrelate_edge` stay raw.
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
