# valence-backend-sql

Shared SQL document-storage helpers used by [`valence-backend-sqlite`](../valence-backend-sqlite/)
and [`valence-backend-postgres`](../valence-backend-postgres/). Reuse JSON-document + edges helpers when building SQL adapters; for application storage, prefer public crate features `sqlite` / `postgres` — **do not depend on this crate directly**.

This is **not** a user-facing engine. There is no `ENGINE_ID` and no public crate feature named `sql`.

See `DatabaseBackend` rustdoc (`cargo doc -p uf-valence-core --open`).
