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

- The in-memory row filter (mem, redis, mongodb, indradb and hybrid backends) now reads
  `LIKE` / `NOT LIKE ... ESCAPE '<c>'` and `IS [NOT] NULL`. Contains, StartsWith and EndsWith
  queries and `where_<field>_is_none()` returned no rows there. `LIKE` honours the escape for
  `%`, `_` and itself and ignores ASCII case, as SQLite does.
- The in-memory row filter's `ORDER BY` (used by the mem, redis, mongodb, indradb and hybrid
  backends) now sorts on every key with its own ASC/DESC, compares numbers numerically
  instead of skipping them, and reads `START n` as an offset.
- Field-level `update` policies are now enforced. `Model::update`, `upsert` and
  `get_mutable().commit()` deny a write that changes a field whose update policy rejects the
  actor (`PrivacyEvaluator::check_field_update_access`). Previously only the table's update
  policy ran, so a field declared `SYSTEM_ONLY` could be rewritten by any actor allowed to
  update the row.

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
- The in-memory backend now evaluates ManyToMany semi-joins, so `where_*_has_results` and
  `where_connection_contains_many_to_many` filter against the backend's own edge store
  instead of being ignored. A semi-join it cannot cut out of the WHERE clause (inside an `OR`
  group) returns an error.
- `DatabaseBackend::apply_batch` and `Valence::apply_batch` apply several writes (`WriteOp`:
  create, update, merge, upsert, delete) as one all-or-nothing unit on a single backend.
  `BackendCapabilities` gains `supports_transactions`; the in-memory backend implements it and
  every other adapter keeps the default "unsupported" error. Batches that span backends are
  rejected. The batch is a raw write: it skips model privacy policies and side effects.
- `valence_core::data_migration`: a `DataMigration` trait and a `MigrationRunner` that
  applies data rewrites in version order. Before anything runs, the runner refuses user
  and anonymous actors, requires a `MigrationApproval` (approver and reason), and requires
  `acknowledge_sensitive(id)` for each migration that touches sensitive data. Each run
  writes an audit row to the append-only `valence_data_migrations` table (actor, approver,
  reason, times, rows touched, outcome); failed runs get their own row with credentials
  redacted, or with only the error kind when the migration touches sensitive data. Run one
  runner per deployment at a time. Applied migrations are skipped on later runs, and an applied migration whose
  version or description changed stops the run. `dry_run(true)` runs the gates and
  `apply` without writing the ledger.
- `TableTransfer` (in `valence_core::backend`) copies a set of tables, then the edges that
  start in them, from one backend to another: SQLite to Postgres is the case it was built
  for. Row ids, history rows and edge identity come across unchanged, along with each
  table's layout and schema version stamp. It pages through `scan_table` / `scan_edges` and
  writes with `upsert_record` / `relate_edge`, so a run that fails partway leaves a
  `TransferCheckpoint` to resume from. `EdgeRow` now derives `Serialize` / `Deserialize`.
  `valence-testkit` gains `run_transfer_contract` and `run_transfer_resume_contract`.
- `valence-testkit` gains `run_platform_transfer_contract`, which transfers a platform-shaped
  set (identity, fleet, workers, placement, permissions, and sealed secrets with a
  hash-chained audit log) and checks row counts, rows, schema stamps, cross-family edges and
  the audit chain per family.
  SQLite-to-SQLite and SQLite-to-Postgres integration tests run it.
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
- `DatabaseBackend::define_index(table, fields)` declares a non-unique lookup index. The
  default does nothing. The mem backend keeps a real equality index for it, and SQLite,
  Postgres and Surreal create one (adding the columns first on a table with no registry
  layout). Wrapping backends should forward it.
- `indexed: true` on a schema or trait field asks the backend for a lookup index on it.
  SQL backends create it with the typed table (bump the schema `version` so boot sync adds
  it to existing tables), and the mem backend indexes `indexed` and `unique` fields when
  boot sync runs.
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
- **Storage contract:** `FieldType::DateTime` is now persisted as UTC unix **milliseconds**
  (was whole seconds), so rows and `updated_at` stamps written in the same second keep
  their order. Reads accept both: an integer with magnitude below 1e11 is treated as legacy
  seconds and anything larger as milliseconds (so an instant before March 1973 written in
  milliseconds reads back as seconds); RFC3339 strings still parse. `DateTimePredicate`
  filters now compare against milliseconds, so a legacy seconds row mis-orders in
  predicates until it is rewritten. Nothing rewrites old rows automatically.

### Fixed

- `valence_codegen::build()` and `generate_models` now return an error when a schema or
  trait file fails codegen, naming the file. They used to print a `cargo:warning` (which
  Cargo hides for git dependencies) and skip the model, so consumers only saw an
  unresolved import.
- A rule in a field `read` policy that is not a sync `PrivacyRule` (for example an async
  `PolicyEvaluator`) now fails the build. Codegen emits a per-rule assertion named after the
  table, field and bucket; before, such a schema compiled and every read of the field failed.
- The pending-deletion check that every query now runs no longer scans the whole
  `valence_data_ownership` table. On the mem backend each check copied every ownership row
  and filtered them, so list reads slowed down as data grew (a one-server Playwright run
  went from about 1 s to over a minute per test). Ownership now defines an index on
  `(valence_model, status)` on each backend the first time it reads ownership rows.
  `ensure_lookup_indexes` uses the same index instead of SurrealQL that only Surreal
  accepted.
- Queued `Model::delete` and `queue_delete_entity` now check for a registered deletion
  dispatcher before marking the row `pending_deletion`. A host without one gets the error
  with the row still visible, instead of a hidden row nothing will purge. The check is
  exposed as `deletion::require_deletion_dispatcher`.
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
