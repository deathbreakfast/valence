# valence-backend-sql

Shared SQL helpers for [`valence-backend-sqlite`](../valence-backend-sqlite/) and
[`valence-backend-postgres`](../valence-backend-postgres/): typed-column DDL/CRUD,
edge junction table, and compiled-query execution.

Schema fields map to real columns (`INTEGER`, `TEXT`, `JSONB`, …). Prefer public
crate features `sqlite` / `postgres` — **do not depend on this crate directly**.

This is **not** a user-facing engine. There is no `ENGINE_ID` and no public crate
feature named `sql`.

See `StorageLayout` and `DatabaseBackend` rustdoc (`cargo doc -p uf-valence-core --open`).
