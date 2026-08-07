//! Deletion DAG node types and cascade ordering.

/// What to do with one row (or edge) in the deletion graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeletionAction {
    /// Physically delete the row.
    CascadeDelete,
    /// Clear an FK field on a referencing row (keep the row).
    SetNull {
        /// Field name to set to JSON `null`.
        field: String,
    },
    /// Remove M2M edges for this endpoint in `edge_table` (keep both rows).
    RemoveEdge { edge_table: String },
}

impl DeletionAction {
    /// Sort key within a depth wave: RemoveEdge → SetNull → CascadeDelete.
    #[must_use]
    pub fn wave_order(&self) -> u8 {
        match self {
            Self::RemoveEdge { .. } => 0,
            Self::SetNull { .. } => 1,
            Self::CascadeDelete => 2,
        }
    }

    /// Optional FK field for SetNull steps.
    #[must_use]
    pub fn set_null_field(&self) -> Option<&str> {
        match self {
            Self::SetNull { field } => Some(field.as_str()),
            _ => None,
        }
    }

    /// Optional edge table for RemoveEdge steps.
    #[must_use]
    pub fn edge_table(&self) -> Option<&str> {
        match self {
            Self::RemoveEdge { edge_table } => Some(edge_table.as_str()),
            _ => None,
        }
    }
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
    /// Sort nodes for execution: deeper first, then RemoveEdge → SetNull → CascadeDelete.
    pub fn sort_for_execution(nodes: &mut [DeletionNode]) {
        nodes.sort_by(|a, b| {
            b.depth
                .cmp(&a.depth)
                .then_with(|| a.action.wave_order().cmp(&b.action.wave_order()))
                .then_with(|| a.table.cmp(&b.table))
                .then_with(|| a.record_id.cmp(&b.record_id))
        });
    }

    #[must_use]
    pub(crate) fn from_nodes(
        root_table: &str,
        root_record_id: &str,
        mut nodes: Vec<DeletionNode>,
        violations: Vec<RestrictViolation>,
    ) -> Self {
        Self::sort_for_execution(&mut nodes);
        Self {
            root_table: root_table.to_string(),
            root_record_id: root_record_id.to_string(),
            nodes,
            restrict_violations: violations,
        }
    }
}
