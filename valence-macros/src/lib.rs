//! Proc macros for Valence schema and trait DSL compilation.
//!
//! # Macros
//!
//! - [`valence_schema!`] — declare a table schema (fields, policies, connections, TTL,
//!   optional per-schema `valence::DatabaseEvaluator`, trait mixins).
//! - [`valence_trait_schema!`] — declare a reusable trait (shared fields / connection names /
//!   policies) that schemas can include via `traits: [TraitName]`.
//!
//! # Pipeline
//!
//! 1. **Parse** — [`valence_schema_dsl`] turns token trees into parsed schema/trait structures.
//! 2. **Emit** — `codegen` builds `valence::Schema`, `valence::TraitDefinition`, and
//!    `inventory::submit!` registrations consumed at runtime by Valence registries.
//!
//! # Where types like `FieldType` live
//!
//! Authoring schemas in app code uses the **`valence`** crate (`valence::prelude::*`), not
//! this crate. `valence-macros` only parses and emits tokens; it does not re-export DSL types.

use proc_macro::TokenStream;

mod codegen;

/// Defines a Valence model schema at compile time.
///
/// Parses the braced DSL, builds a `valence::Schema`, and registers
/// `valence::SchemaMetadataInit` plus optional `valence::TraitImplementor` entries for
/// each `traits: [...]` name.
///
/// # DSL fields
///
/// | Field | Required | Description |
/// |-------|----------|-------------|
/// | `table` | yes | Physical / registry table name |
/// | `version` | yes | Schema version string |
/// | `fields` | yes | Named field list (`r#type`, `primary_key`, `required`, …). See field-type notes below. |
/// | `description` | no | Human-readable summary |
/// | `database` | no | Path to a `const`/`static` `DatabaseEvaluator`; defaults to `DEFAULT_IN_MEMORY` |
/// | `policies` | no | Read/write/update/delete allow lists |
/// | `connections` | no | Graph/FK edges (`Cardinality`, `OnDelete`) |
/// | `ttl` | no | Create-only TTL (`ttl: { seconds }`); native Redis/Mongo; Deferred backends stamp `__valence_expire_at`, ensure an expire-at index, and warn until `valence_platform::ttl_sweep::register_ttl_service`; boot with `Valence::ensure_ttl_for_all` (see `valence::ttl`) |
/// | `side_effects` | no | Mutation hooks |
/// | `composite_key` | no | Multi-field primary key |
/// | `traits` | no | Mix in `valence_trait_schema!` names |
///
/// Legacy `privacy: { ... }` blocks are accepted by the parser for compatibility but do not
/// change emitted runtime metadata (read/write privacy strings are fixed in codegen).
///
/// ## Field types (`r#type`)
///
/// - `FieldType::Json` — `serde_json::Value`
/// - `FieldType::JsonAs("path::Type")` — typed JSON; constructors/setters take `T`; optional
///   `.serde_error(JsonAsSerdeError::Panic|Error)` (default `Error`)
/// - `FieldType::Record("table")` — `RecordId`; optional `.target("path::Model")` for hops
/// - `FieldType::Currency` — `valence::Currency` (`CurrencyCode` + `amount_minor`)
/// - `FieldType::DateTime` — Model API `chrono::DateTime<Utc>`; storage is **UTC unix seconds**
///
/// Connections accept `model:` or `target:` (aliases) for an explicit model path.
///
/// Backend selection uses a stable evaluator rather than a backend instance:
///
/// ```ignore
/// const COUNTER_DB: valence::DatabaseFromEngine =
///     valence::Database::from_engine("default", valence::SQLITE_ENGINE_ID);
///
/// valence_schema! {
///     Counter {
///         table: "counter",
///         version: "0.1.0",
///         database: COUNTER_DB,
///         fields: [],
///     }
/// }
/// ```
///
/// The logical name (`"default"`) must match the name passed to
/// `ValenceBuilder::add_backend`. When `database:` is omitted, the schema evaluator is
/// `DEFAULT_IN_MEMORY`; the runtime may fall back to its active backend if that key is absent.
///
/// # Examples
///
/// ```ignore
/// use valence::prelude::*;
///
/// const COUNTER_DB: DatabaseFromEngine =
///     Database::from_engine("default", valence::MEM_ENGINE_ID);
///
/// valence_schema! {
///     Counter {
///         table: "counter",
///         version: "0.1.0",
///         description: "Simple counter",
///         database: COUNTER_DB,
///         fields: [
///             id: { r#type: FieldType::String, primary_key: true, required: true },
///             value: { r#type: FieldType::Integer, required: true, default: 0 },
///         ]
///     }
/// }
/// ```
///
/// This registers table `counter` via inventory. Model CRUD requires `valence-codegen`
/// (`examples/codegen-host`).
#[proc_macro]
pub fn valence_schema(input: TokenStream) -> TokenStream {
    codegen::schema::expand(input)
}

/// Defines a reusable trait schema (fields, optional connections, optional policies).
///
/// Registered as `valence::TraitDefinitionInit` so `valence::TraitRegistry` can merge
/// trait requirements with concrete `valence_schema!` tables that list `traits: [ThisTrait]`.
///
/// # DSL fields
///
/// | Field | Required | Description |
/// |-------|----------|-------------|
/// | `fields` | no | Reusable named field definitions; uses the same attributes as [`valence_schema!`] |
/// | `connections` | no | Reusable connection names; concrete schemas provide full edge metadata |
/// | `policies` | no | Reusable `read`/`create`/`update`/`delete` policy bundles |
///
/// At least one of these fields should be supplied for the trait to add behavior.
///
/// ## Field attributes
///
/// | Attribute | Required | Description |
/// |-----------|----------|-------------|
/// | `r#type` (or `type`) | yes | `FieldType` expression. Supported variants include `String`, `Integer`, `Boolean`, `DateTime` (Model API `DateTime<Utc>`; storage is **UTC unix seconds**), `Json` (`serde_json::Value`), `JsonAs("path::Type")` (typed JSON; optional `.serde_error(JsonAsSerdeError::Panic\|Error)`, default `Error`), `Record("table")` (optional `.target("path::Model")` for connection hops), `Currency` (`valence::Currency` / ISO-4217 `CurrencyCode`), `Enum` / `ExternalEnum`. |
/// | `required` | no | Whether the field must be present (default `false`) |
/// | `primary_key` | no | Whether the field is a primary key (default `false`) |
/// | `unique` | no | Whether values must be unique (default `false`) |
/// | `default` | no | Default value expression |
/// | `validations` | no | Validator expression list |
/// | `policies` | no | Field-level policy bundle |
/// | `encrypted` | no | Whether storage should treat the field as encrypted (default `false`) |
///
/// ## Policy fields
///
/// A `policies` block accepts `read`, `create`, `update`, and `delete`. Each operation accepts
/// `always_allow`, `allow`, `block`, and `always_block` rule lists.
///
/// ## Connection fields
///
/// A `connections` entry has a name and braced expression body. Trait registration preserves
/// the connection name for attribution; declare full connection metadata (`table`,
/// `cardinality`, `required`, `on_delete`, and related attributes) on the concrete schema.
/// Use `model:` or `target:` (aliases) for an explicit cross-crate model path.
///
/// # Examples
///
/// ```ignore
/// use valence::prelude::*;
///
/// valence_trait_schema! {
///     Owned {
///         fields: [
///             owner: {
///                 r#type: FieldType::Record("user"),
///                 required: true,
///                 policies: { read: { allow: [AUTHENTICATED] } },
///             },
///         ],
///         connections: [
///             owner: { table: "user", cardinality: HasOne, on_delete: Cascade },
///         ],
///         policies: {
///             read: { allow: [AUTHENTICATED] },
///         },
///     }
/// }
///
/// valence_schema! {
///     Person {
///         table: "person",
///         version: "0.1.0",
///         traits: [Owned],
///         fields: [
///             id: { r#type: FieldType::String, primary_key: true, required: true },
///         ],
///     }
/// }
/// ```
#[proc_macro]
pub fn valence_trait_schema(input: TokenStream) -> TokenStream {
    codegen::trait_schema::expand(input)
}
