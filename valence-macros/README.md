# valence-macros

Procedural macros for Valence schemas — use `valence_schema!` and `valence_trait_schema!` through the `valence` crate. Macro expansion and DSL syntax are documented here for workspace development.

## `valence_schema!`

Defines a schema using a Rust DSL. The macro registers schema metadata into
`SchemaRegistry` at module initialization time.

### DSL fields

| Field | Required | Description |
|-------|----------|-------------|
| `table` | yes | Physical / registry table name |
| `version` | yes | Schema version string |
| `fields` | yes | Named field list |
| `description` | no | Human-readable summary |
| `database` | no | Path to a `DatabaseEvaluator` const/static; defaults to `DEFAULT_IN_MEMORY` |
| `policies` | no | Read/write/update/delete allow lists |
| `connections` | no | Graph/FK edges. A `ManyToMany` connection can set `edge_policy` so edge writes are privacy-checked; see [Checked many-to-many edges](#checked-many-to-many-edges). |
| `ttl` | no | Create-only time-to-live (`ttl: { seconds: N }`). Native on Redis/Mongo; Deferred backends stamp `__valence_expire_at`, ensure an expire-at index, and warn until `valence_platform::ttl_sweep::register_ttl_service` is wired. Call `Valence::ensure_ttl_for_all()` once at boot. See `valence::ttl`. |
| `side_effects` | no | Mutation hooks |
| `composite_key` | no | Multi-field primary key |
| `traits` | no | Mix in `valence_trait_schema!` names |
| `retired_fields` | no | Names of removed fields whose columns stay in the table (`retired_fields: ["project"]`). See [Retiring a field](#retiring-a-field). |

### Backend selection

`database:` takes an evaluator constant (a `DatabaseFromEngine` value), then you register the matching adapter by logical name:

```rust
use valence::{Database, DatabaseFromEngine, SQLITE_ENGINE_ID};

const COUNTER_DB: DatabaseFromEngine =
    Database::from_engine("default", SQLITE_ENGINE_ID);
```

Use `database: COUNTER_DB` in the schema and register the SQLite adapter with
`.add_backend("default", …)`. The logical name must match; the engine constant changes by adapter.
If `database:` is omitted, the schema uses `DEFAULT_IN_MEMORY` (and the runtime currently falls
back to its active backend when that router key is absent).

### Field attributes (common)

| Attribute | Meaning |
|-----------|---------|
| `r#type` | `String`, `Integer`, `Float`, `Boolean`, … (or `FieldType::…`) |
| `primary_key` | Mark primary key column |
| `required` | Non-optional field |
| `default` | Default value expression |

### DSL syntax (recommended)

```rust
use valence::prelude::*;

const COUNTER_DB: DatabaseFromEngine =
    Database::from_engine("default", valence::MEM_ENGINE_ID);

valence_schema! {
    Counter {
        table: "counter",
        version: "0.1.0",
        description: "Simple counter",
        database: COUNTER_DB,
        policies: { read: { allow: [PUBLIC_READ] } },
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            value: { r#type: FieldType::Integer, required: true },
        ]
    }
}
```

### Retiring a field

Boot sync refuses to start when a typed SQL table has a column the schema no longer
declares, so a typo can't silently orphan data. To remove a field on purpose, delete it
from `fields`, list its name under `retired_fields`, and bump `version` so sync runs:

```rust
valence_schema! {
    Task {
        table: "task",
        version: "0.3.0",
        repository: "https://github.com/your-org/your-app",
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            title: { r#type: FieldType::String, required: true },
        ],
        // `project` was a field until 0.2.0.
        retired_fields: ["project"],
    }
}
```

On SQLite and Postgres the column stays in the table and sync runs no DDL for it. Reads
leave it out (`get`, queries, and the rows privacy policies see), and writes never list it,
so new rows hold NULL there. Old rows keep their value on disk until you drop the column
yourself.

If the retired column is `NOT NULL` without a `DEFAULT`, new inserts would fail. Postgres
sync relaxes the column to nullable; SQLite sync refuses with an error naming the column,
because SQLite can't relax `NOT NULL` without rebuilding the table.

To undo a retirement, remove the name from `retired_fields` and add the field back. The
macro rejects a name that is still in `fields`, `id`, a duplicate, or a name that isn't a
valid identifier. Document backends (mem, Surreal, MongoDB, Redis, IndraDB) have no fixed
columns, so they accept the key and ignore it.

### Checked many-to-many edges

`Valence::relate_edge` and `unrelate_edge` write edges without privacy checks unless the
connection opts in. Set `edge_policy: SourceUpdateTargetRead` on a `ManyToMany` connection
and every relate or unrelate on its `edge_table` is refused unless the actor may update the
source row and read the target row:

```rust
valence_schema! {
    Doc {
        table: "doc",
        version: "0.2.0",
        repository: "https://github.com/your-org/your-app",
        policies: {
            read: { allow: [valence::privacy_policies::common::AUTHENTICATED] },
            update: { allow: [valence::privacy_policies::owner::OWNER_BY_USER_FIELD] },
            // create, delete ...
        },
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            user: { r#type: FieldType::String, required: true },
        ],
        connections: [
            labels: {
                table: "label",
                cardinality: ManyToMany,
                edge_table: "doc_label",
                edge_policy: SourceUpdateTargetRead,
                on_delete: Cascade,
                model: "crate::generated::Label",
            },
        ],
    }
}
```

Valence loads the stored `from` row and runs the source schema's Update policy on it, then
loads the `to` row and runs its own schema's Read policy. A missing row on either side
returns `Error::NotFound` and a denial returns `Error::Privacy`; in both cases nothing is
written. The generated `relate_to_label` / `unrelate_from_label` call `relate_edge`, so they
are checked too. Bypasses come only from the policies themselves, such as
`always_allow: [SYSTEM_ONLY]`.

Unrelate uses the same rule, so an actor who can only read a doc can't remove its links.
Edge cleanup during record deletion is not checked; the deleted row already passed Delete.
Any other cardinality or value is a compile error. Connections without `edge_policy` keep
the raw write, and the host must authorize those calls itself.

## `valence_trait_schema!`

Reusable field/policy bundles mixed into schemas via `traits: [TraitName]`.

### DSL fields

| Field | Required | Description |
|-------|----------|-------------|
| `fields` | no | Reusable named field definitions; same attributes as `valence_schema!` |
| `connections` | no | Reusable connection names; full edge metadata belongs on concrete schemas |
| `policies` | no | Reusable `read`/`create`/`update`/`delete` policy bundles |

At least one should be supplied for the trait to add behavior.

### Trait field attributes

| Attribute | Required | Description |
|-----------|----------|-------------|
| `r#type` (or `type`) | yes | `FieldType` expression |
| `required` | no | Require the field (default `false`) |
| `primary_key` | no | Mark as primary key (default `false`) |
| `unique` | no | Require unique values (default `false`). SQL backends enforce it with a unique index; a duplicate write fails with `Error::unique_violation`, which callers match through `Error::as_unique_violation` |
| `default` | no | Default value expression |
| `validations` | no | Validator expression list |
| `policies` | no | Field-level policy bundle |
| `encrypted` | no | Mark field as encrypted (default `false`) |

Policy blocks accept `read`, `create`, `update`, and `delete`; each operation accepts
`always_allow`, `allow`, `block`, and `always_block` rule lists.

```rust
use valence::prelude::*;

valence_trait_schema! {
    Owned {
        fields: [
            owner: {
                r#type: FieldType::Record("user"),
                required: true,
                policies: { read: { allow: [AUTHENTICATED] } },
            },
        ],
        connections: [
            owner: { table: "user", cardinality: HasOne },
        ],
        policies: {
            read: { allow: [AUTHENTICATED] },
        },
    }
}
```

Trait registration retains connection names for attribution. Define complete connection
metadata on the concrete `valence_schema!`.

## What the macros do

- Parse DSL via shared [`valence-schema-dsl`](../valence-schema-dsl/README.md) (same grammar as codegen).
- Extract table name/version/description.
- Register a `SchemaMetadata` entry in the global `SchemaRegistry`.

The macros do **not** generate models. Model generation is handled by the
[`valence-codegen`](../valence-codegen/README.md) build step.

See also:

- [`examples/minimal-schema/`](../examples/minimal-schema/) — schema macro expansion without codegen
- [`examples/codegen-host/`](../examples/codegen-host/) — end-to-end codegen pipeline
- Crate rustdoc — full DSL tables and examples
