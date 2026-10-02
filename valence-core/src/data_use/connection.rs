//! Connection hops attributed to a declared data use.

/// Kind of connection traversal a declared use performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionHopKind {
    /// Forward `get_{field}` / `get_{field}_record_ids` load of the peer record.
    ForwardGet,
    /// `relate_to_*` / `unrelate_from_*` edge mutation.
    Relate,
}

/// Connection a declared use loads or mutates, so the peer schema's Data uses
/// page can list it under Referenced Reads / Updates.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ConnectionHop {
    /// Connection field token taken from the method name (`owner` for `get_owner`).
    pub field: String,
    /// Forward load or edge mutation.
    pub kind: ConnectionHopKind,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_hop_round_trips_serde() {
        let hop = ConnectionHop {
            field: "owner".into(),
            kind: ConnectionHopKind::ForwardGet,
        };
        let json = serde_json::to_string(&hop).unwrap();
        assert_eq!(json, r#"{"field":"owner","kind":"forward_get"}"#);
        let back: ConnectionHop = serde_json::from_str(&json).unwrap();
        assert_eq!(back, hop);

        let relate: ConnectionHopKind = serde_json::from_str(r#""relate""#).unwrap();
        assert_eq!(relate, ConnectionHopKind::Relate);
        assert!(serde_json::from_str::<ConnectionHopKind>(r#""sideways""#).is_err());
    }
}
