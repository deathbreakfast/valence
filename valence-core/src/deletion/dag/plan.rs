//! Deletion DAG node types and cascade ordering.

/// What to do with one row in the deletion graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeletionAction {
    CascadeDelete,
}

#[derive(Debug, Clone)]
pub struct DeletionNode {
    pub table: String,
    pub record_id: String,
    pub action: DeletionAction,
    pub depth: u32,
    pub connection_name: String,
    pub from_table: String,
}

#[derive(Debug, Clone)]
pub struct RestrictViolation {
    pub blocking_table: String,
    pub blocking_field: String,
    pub blocking_record_count: i64,
    pub connection_name: String,
}

#[derive(Debug, Clone)]
pub struct DeletionDag {
    pub root_table: String,
    pub root_record_id: String,
    pub nodes: Vec<DeletionNode>,
    pub restrict_violations: Vec<RestrictViolation>,
}

impl DeletionDag {
    #[must_use]
    pub(crate) fn nodes_from_visited(
        root_table: &str,
        root_record_id: &str,
        visited: std::collections::HashMap<(String, String), u32>,
        violations: Vec<RestrictViolation>,
    ) -> Self {
        let mut entries: Vec<(String, String, u32)> =
            visited.into_iter().map(|((t, r), d)| (t, r, d)).collect();
        entries.sort_by(|a, b| {
            let by_depth = b.2.cmp(&a.2);
            if by_depth != std::cmp::Ordering::Equal {
                return by_depth;
            }
            a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1))
        });

        let nodes: Vec<DeletionNode> = entries
            .into_iter()
            .map(|(t, r, d)| DeletionNode {
                table: t.clone(),
                record_id: r,
                action: DeletionAction::CascadeDelete,
                depth: d,
                connection_name: "cascade".to_string(),
                from_table: t,
            })
            .collect();

        Self {
            root_table: root_table.to_string(),
            root_record_id: root_record_id.to_string(),
            nodes,
            restrict_violations: violations,
        }
    }
}
