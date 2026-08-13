//! Quote the schema metadata accessor struct and trait impl.

use proc_macro2::TokenStream;
use quote::quote;

use super::collect::SchemaMetadataPieces;

pub(super) fn quote_schema_metadata_method(p: &SchemaMetadataPieces) -> TokenStream {
    let struct_name = &p.struct_name;
    let schema_struct_name = &p.schema_struct_name;
    let version_lit = &p.version_lit;
    let read_lit = &p.read_lit;
    let write_lit = &p.write_lit;
    let table_name_lit = &p.table_name_lit;
    let schema_fields = &p.schema_fields;
    let edges = &p.edges;
    let connections = &p.connections;
    let description_code = &p.description_code;
    let description_const_code = &p.description_const_code;
    let policies_code = &p.policies_code;
    let trait_names_code = &p.trait_names_code;
    let side_effects_code = &p.side_effects_code;
    let iters_code = &p.iters_code;
    let ttl_code = &p.ttl_code;
    let composite_key_code = &p.composite_key_code;
    let ownership_code = &p.ownership_code;
    let database_evaluator_code = &p.database_evaluator_code;
    let database_typecheck_code = &p.database_typecheck_code;

    quote! {
        /// Schema metadata for #struct_name - zero-heap scalar access
        #[allow(dead_code)]
        pub struct #schema_struct_name;

        #database_typecheck_code

        #[allow(dead_code)]
        impl #schema_struct_name {
            /// Table name (zero allocation)
            pub const fn name() -> &'static str {
                #table_name_lit
            }

            /// Schema version (zero allocation)
            pub const fn version() -> &'static str {
                #version_lit
            }

            /// Table name for database operations (zero allocation)
            pub const fn table_name() -> &'static str {
                #table_name_lit
            }

            /// Human-readable description (zero allocation)
            pub const fn description() -> Option<&'static str> {
                #description_const_code
            }

            /// Privacy read level (zero allocation)
            pub const fn privacy_read() -> &'static str {
                #read_lit
            }

            /// Privacy write level (zero allocation)
            pub const fn privacy_write() -> &'static str {
                #write_lit
            }

            /// Full schema with all fields, edges, and policies
            /// Constructed once on first access, then cached statically
            pub fn full() -> &'static valence::Schema {
                use std::sync::OnceLock;
                static SCHEMA: OnceLock<valence::Schema> = OnceLock::new();
                SCHEMA.get_or_init(|| {
                    let __valence_db_eval: &'static dyn valence::DatabaseEvaluator =
                        #database_evaluator_code;
                    valence::Schema {
                        name: Self::name().to_string(),
                        version: Self::version().to_string(),
                        database_evaluator: __valence_db_eval,
                        databases: vec![__valence_db_eval.name().to_string()],
                        privacy: valence::SchemaPrivacy {
                            read: Self::privacy_read().to_string(),
                            write: Self::privacy_write().to_string(),
                        },
                        policies: #policies_code,
                        fields: vec![
                            #(#schema_fields),*
                        ],
                        edges: vec![
                            #(#edges),*
                        ],
                        connections: vec![
                            #(#connections),*
                        ],
                        side_effects: #side_effects_code,
                        iters: #iters_code,
                        composite_key: #composite_key_code,
                        traits: #trait_names_code,
                        ttl: #ttl_code,
                        ownership: #ownership_code,
                        meta: valence::SchemaMeta {
                            retention: "365 days".to_string(),
                            row_count: 0,
                            owner: "system".to_string(),
                            description: #description_code,
                        },
                    }
                })
            }

            /// Lightweight schema metadata for registry lookups
            pub fn metadata() -> &'static valence::SchemaMetadataStruct {
                use std::sync::OnceLock;
                static METADATA: OnceLock<valence::SchemaMetadataStruct> = OnceLock::new();
                let m = METADATA.get_or_init(|| {
                    let schema = Self::full();
                    valence::SchemaMetadataStruct {
                        table_name: schema.name.as_str(),
                        version: schema.version.as_str(),
                        description: schema.meta.description.as_deref(),
                        privacy_read: schema.privacy.read.as_str(),
                        privacy_write: schema.privacy.write.as_str(),
                        databases: schema.databases.as_slice(),
                        schema,
                    }
                });
                m
            }

            /// Physical storage layout derived from [`Self::full`] (typed columns / fields).
            ///
            /// # Errors
            ///
            /// Returns [`valence::Error::Validation`] for unsafe identifiers or unknown types.
            pub fn storage_layout() -> valence::Result<valence::StorageLayout> {
                valence::StorageLayout::from_schema(Self::full())
            }
        }

        impl valence::SchemaMetadata for #struct_name {
            type SchemaMetadata = valence::Schema;

            fn schema_metadata() -> &'static Self::SchemaMetadata {
                #schema_struct_name::full()
            }
        }

    }
}
