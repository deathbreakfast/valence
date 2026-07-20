//! ManyToMany: concrete model targets vs trait targets (`RecordId`-based).

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use valence_core::SchemaConnection;

use super::common::connection_target_type;

pub(super) fn push_many_to_many_concrete_methods(
    conn: &SchemaConnection,
    methods: &mut Vec<TokenStream>,
) {
    let Some(edge_table) = conn.edge_table.as_ref() else {
        return;
    };
    let conn_name = &conn.from_field;
    let target_type = connection_target_type(conn);

    let get_method_name = format_ident!("get_{}", conn_name);
    let edge_table_lit = edge_table.as_str();
    let to_table_lit = conn.to_table.as_str();

    methods.push(quote! {
        /// Navigate the `#conn_name` connection (ManyToMany). Loads related records via edge table, runs read privacy.
        pub async fn #get_method_name(&self, valence: &valence::Valence) -> valence::Result<Vec<#target_type>> {
            let from_rid = self.id().ok_or_else(|| valence::Error::Validation("Record has no id".into()))?;
            valence.get_many_to_many_targets(&from_rid, #edge_table_lit, #to_table_lit).await
        }
    });

    let singular = conn_name.strip_suffix('s').unwrap_or(conn_name);
    let relate_name = format_ident!("relate_to_{}", singular);
    let unrelate_name = format_ident!("unrelate_from_{}", singular);

    methods.push(quote! {
        /// Create a ManyToMany edge between this record and the target.
        pub async fn #relate_name(&self, target: &#target_type, valence: &valence::Valence) -> valence::Result<()> {
            let from_rid = self.id().ok_or_else(|| valence::Error::Validation("Record has no id".into()))?;
            let to_rid = target.id().ok_or_else(|| valence::Error::Validation("Target has no id".into()))?;
            valence.relate_edge(#edge_table_lit, &from_rid, &to_rid).await
        }
    });
    methods.push(quote! {
        /// Remove the ManyToMany edge between this record and the target.
        pub async fn #unrelate_name(&self, target: &#target_type, valence: &valence::Valence) -> valence::Result<()> {
            let from_rid = self.id().ok_or_else(|| valence::Error::Validation("Record has no id".into()))?;
            let to_rid = target.id().ok_or_else(|| valence::Error::Validation("Target has no id".into()))?;
            valence.unrelate_edge(#edge_table_lit, &from_rid, &to_rid).await
        }
    });
}

pub(super) fn push_many_to_many_trait_target_methods(
    conn: &SchemaConnection,
    methods: &mut Vec<TokenStream>,
) {
    let Some(edge_table) = conn.edge_table.as_ref() else {
        return;
    };
    let conn_name = &conn.from_field;
    let get_method_name = format_ident!("get_{}_record_ids", conn_name);
    let edge_table_lit = edge_table.as_str();
    let target_trait_lit = conn.target_trait.as_deref().unwrap_or_default();

    methods.push(quote! {
        /// Navigate the `#conn_name` connection (ManyToMany trait target).
        /// Returns target [`valence::RecordId`] values so callers can resolve concrete models by table.
        pub async fn #get_method_name(&self, valence: &valence::Valence) -> valence::Result<Vec<valence::RecordId>> {
            let from_rid = self.id().ok_or_else(|| valence::Error::Validation("Record has no id".into()))?;
            let allowed_tables = valence::TraitRegistry::global()
                .tables_for_trait(#target_trait_lit)
                .into_iter()
                .map(str::to_string)
                .collect::<std::collections::HashSet<_>>();
            let ids = valence.get_many_to_many_target_record_ids(&from_rid, #edge_table_lit).await?;
            Ok(ids
                .into_iter()
                .filter(|rid| allowed_tables.contains(rid.table()))
                .collect())
        }
    });

    let singular = conn_name.strip_suffix('s').unwrap_or(conn_name);
    let relate_name = format_ident!("relate_to_{}_record", singular);
    let unrelate_name = format_ident!("unrelate_from_{}_record", singular);

    methods.push(quote! {
        /// Create a ManyToMany edge between this record and a trait target record.
        pub async fn #relate_name(&self, target: &valence::RecordId, valence: &valence::Valence) -> valence::Result<()> {
            let from_rid = self.id().ok_or_else(|| valence::Error::Validation("Record has no id".into()))?;
            let allowed_tables = valence::TraitRegistry::global().tables_for_trait(#target_trait_lit);
            if !allowed_tables.iter().any(|table| *table == target.table()) {
                return Err(valence::Error::Validation(format!(
                    "Target table '{}' does not implement trait '{}'",
                    target.table(),
                    #target_trait_lit
                )));
            }
            valence.relate_edge(#edge_table_lit, &from_rid, target).await
        }
    });
    methods.push(quote! {
        /// Remove the ManyToMany edge between this record and a trait target record.
        pub async fn #unrelate_name(&self, target: &valence::RecordId, valence: &valence::Valence) -> valence::Result<()> {
            let from_rid = self.id().ok_or_else(|| valence::Error::Validation("Record has no id".into()))?;
            valence.unrelate_edge(#edge_table_lit, &from_rid, target).await
        }
    });
}
