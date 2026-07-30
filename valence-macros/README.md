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
| `connections` | no | Graph/FK edges |
| `ttl` | no | Time-to-live policy |
| `side_effects` | no | Mutation hooks |
| `composite_key` | no | Multi-field primary key |
| `traits` | no | Mix in `valence_trait_schema!` names |

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
| `unique` | no | Require unique values (default `false`) |
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
