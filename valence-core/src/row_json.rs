//! JSON row helpers shared by query fetch paths.

/// Strip Surreal thing decorations to a bare id segment.
pub fn thing_to_id_only(mut s: String) -> String {
    s = s.split(':').next_back().unwrap_or(&s).to_string();
    s = s
        .replace(['⟩', '⟨', '›', '‹', '»', '«'], "")
        .trim()
        .to_string();
    // SurrealDB 3 wraps ids that need escaping (hyphenated UUIDs) in backticks.
    if let Some(bare) = s.strip_prefix('`').and_then(|rest| rest.strip_suffix('`')) {
        s = bare.to_string();
    }
    s
}

/// Coerce a string or bare `id` field into the `{ table, id }` shape generated models expect.
pub fn normalize_record_id_field(table: &str, value: &mut serde_json::Value) {
    let Some(obj) = value.as_object_mut() else {
        return;
    };
    let Some(id_val) = obj.get("id").cloned() else {
        return;
    };
    if let Some(id_str) = id_val.as_str() {
        let bare = thing_to_id_only(id_str.to_string());
        obj.insert(
            "id".into(),
            serde_json::json!({ "table": table, "id": bare }),
        );
    }
}

/// JSON object form of [`crate::RecordId`] for adapter write responses.
pub fn record_id_json(table: &str, id: &str) -> serde_json::Value {
    serde_json::json!({
        "table": table,
        "id": id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thing_to_id_only_strips_backticks_happy() {
        let uuid = "aa90d993-a8e9-4433-bdcb-db51b35ceb9d";
        assert_eq!(thing_to_id_only(format!("team:`{uuid}`")), uuid);
        assert_eq!(thing_to_id_only("team:⟨abc⟩".into()), "abc");
        let mut row = serde_json::json!({"id": format!("team:`{uuid}`")});
        normalize_record_id_field("team", &mut row);
        assert_eq!(row["id"], serde_json::json!({"table": "team", "id": uuid}));
    }

    #[test]
    fn thing_to_id_only_keeps_lone_backtick_sad() {
        assert_eq!(thing_to_id_only("team:`abc".into()), "`abc");
    }
}
