use super::*;

#[test]
fn filters_json_extract_equality_param() {
    let compiled = CompiledQuery::new(
        "SELECT id, body FROM project WHERE json_extract(body, '$.name') = $param_0".into(),
        vec![("param_0".into(), serde_json::json!("alpha"))],
    );
    let rows = vec![
        serde_json::json!({"id": "1", "name": "alpha"}),
        serde_json::json!({"id": "2", "name": "beta"}),
    ];
    let out = apply_equality_where(rows, &compiled);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0]["name"], "alpha");
}

#[test]
fn filters_bare_column_equality_param() {
    let compiled = CompiledQuery::new(
        "SELECT * FROM project WHERE name = $param_0".into(),
        vec![("param_0".into(), serde_json::json!("alpha"))],
    );
    let rows = vec![
        serde_json::json!({"id": {"table": "project", "id": "1"}, "name": "alpha"}),
        serde_json::json!({"id": {"table": "project", "id": "2"}, "name": "beta"}),
    ];
    let out = apply_equality_where(rows, &compiled);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0]["name"], "alpha");
}

#[test]
fn filters_record_or_clause_against_object_fk() {
    let compiled = CompiledQuery::new(
        "SELECT id, body FROM task WHERE (json_extract(body, '$.project') = $param_0 OR json_extract(body, '$.project') = $param_1 OR json_extract(body, '$.project.id') = $param_1)".into(),
        vec![
            ("param_0".into(), serde_json::json!("project:p1")),
            ("param_1".into(), serde_json::json!("p1")),
        ],
    );
    let rows = vec![
        serde_json::json!({"id": "t1", "project": {"table": "project", "id": "p1"}}),
        serde_json::json!({"id": "t2", "project": {"table": "project", "id": "other"}}),
    ];
    let out = apply_equality_where(rows, &compiled);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0]["id"], "t1");
}

#[test]
fn filters_datetime_after_numeric_param() {
    let compiled = CompiledQuery::new(
        "SELECT id, body FROM typed_probe WHERE json_extract(body, '$.at') > $param_0".into(),
        vec![("param_0".into(), serde_json::json!(2_000_000_000_i64))],
    );
    let rows = vec![
        serde_json::json!({"id": "1", "at": 1_700_000_000_i64}),
        serde_json::json!({"id": "2", "at": 2_100_000_000_i64}),
    ];
    let out = apply_equality_where(rows, &compiled);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0]["id"], "2");
}

#[test]
fn filters_datetime_after_miss_when_all_before_threshold() {
    let compiled = CompiledQuery::new(
        "SELECT id, body FROM typed_probe WHERE json_extract(body, '$.at') > $param_0".into(),
        vec![("param_0".into(), serde_json::json!(3_000_000_000_i64))],
    );
    let rows = vec![serde_json::json!({"id": "1", "at": 1_700_000_000_i64})];
    let out = apply_equality_where(rows, &compiled);
    assert_eq!(out, Vec::<serde_json::Value>::new());
}

#[test]
fn extract_field_ref_typed_column_json_extract() {
    assert_eq!(
        extract_field_ref("json_extract(price, '$.code')"),
        "price.code"
    );
    assert_eq!(
        extract_field_ref("CAST(json_extract(price, '$.amount_minor') AS INTEGER)"),
        "price.amount_minor"
    );
    assert_eq!(extract_field_ref("json_extract(body, '$.name')"), "name");
}

#[test]
fn apply_equality_where_currency_code_hit() {
    let compiled = CompiledQuery::new(
        "SELECT * FROM typed_probe WHERE json_extract(price, '$.code') = $param_0".into(),
        vec![("param_0".into(), serde_json::json!("USD"))],
    );
    let rows = vec![
        serde_json::json!({"id": "1", "price": {"code": "USD", "amount_minor": 12345}}),
        serde_json::json!({"id": "2", "price": {"code": "EUR", "amount_minor": 99}}),
    ];
    let out = apply_equality_where(rows, &compiled);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0]["id"], "1");
}

#[test]
fn apply_equality_where_currency_code_miss() {
    let compiled = CompiledQuery::new(
        "SELECT * FROM typed_probe WHERE json_extract(price, '$.code') = $param_0".into(),
        vec![("param_0".into(), serde_json::json!("JPY"))],
    );
    let rows =
        vec![serde_json::json!({"id": "1", "price": {"code": "USD", "amount_minor": 12345}})];
    let out = apply_equality_where(rows, &compiled);
    assert_eq!(out, Vec::<serde_json::Value>::new());
}

#[test]
fn apply_equality_where_currency_minor_cast_hit() {
    let compiled = CompiledQuery::new(
        "SELECT * FROM typed_probe WHERE CAST(json_extract(price, '$.amount_minor') AS INTEGER) = $param_0".into(),
        vec![("param_0".into(), serde_json::json!(12345_i64))],
    );
    let rows = vec![
        serde_json::json!({"id": "1", "price": {"code": "USD", "amount_minor": 12345}}),
        serde_json::json!({"id": "2", "price": {"code": "USD", "amount_minor": 1}}),
    ];
    let out = apply_equality_where(rows, &compiled);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0]["id"], "1");
}

#[test]
fn apply_equality_where_currency_code_and_minor_and_clause() {
    let compiled = CompiledQuery::new(
        "SELECT * FROM typed_probe WHERE json_extract(price, '$.code') = $param_0 AND CAST(json_extract(price, '$.amount_minor') AS INTEGER) = $param_1".into(),
        vec![
            ("param_0".into(), serde_json::json!("USD")),
            ("param_1".into(), serde_json::json!(12345_i64)),
        ],
    );
    let rows = vec![
        serde_json::json!({"id": "1", "price": {"code": "USD", "amount_minor": 12345}}),
        serde_json::json!({"id": "2", "price": {"code": "USD", "amount_minor": 1}}),
    ];
    let out = apply_equality_where(rows, &compiled);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0]["id"], "1");
}

#[test]
fn count_where_thing_eq_or_clause_filters_record_fk() {
    // Exact shape emitted by `compiled_query_factory::count_where_thing_eq` for SQL/mem.
    let compiled = CompiledQuery::new(
        "SELECT COUNT(*) AS count FROM account \
         WHERE json_extract(body, '$.user') = $bare_id \
            OR json_extract(body, '$.user.id') = $bare_id \
            OR json_extract(body, '$.user') = $parent_rid"
            .into(),
        vec![
            ("bare_id".into(), serde_json::json!("persona")),
            ("parent_rid".into(), serde_json::json!("user:persona")),
        ],
    );
    let rows = vec![
        serde_json::json!({"id": "a1", "user": {"table": "user", "id": "owner"}}),
        serde_json::json!({"id": "a2", "user": {"table": "user", "id": "persona"}}),
    ];
    let out = apply_equality_where(rows, &compiled);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0]["id"], "a2");
}

#[test]
fn filters_like_contains_pattern_on_bare_column() {
    let compiled = CompiledQuery::new(
        "SELECT * FROM doc WHERE title LIKE $param_0".into(),
        vec![("param_0".into(), serde_json::json!("%Beacon%"))],
    );
    let rows = vec![
        serde_json::json!({"id": "1", "title": "UniqueBeaconMatch"}),
        serde_json::json!({"id": "2", "title": "Other"}),
    ];
    let out = apply_equality_where(rows, &compiled);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0]["title"], "UniqueBeaconMatch");
}

#[test]
fn filters_search_or_like_across_title_and_searchable_text() {
    // Shape emitted by QueryCore::search + set_search_fields for SQL/mem.
    let compiled = CompiledQuery::new(
        "SELECT * FROM unified_field_search_document WHERE (title LIKE $param_0 OR searchable_text LIKE $param_1)".into(),
        vec![
            ("param_0".into(), serde_json::json!("%CapMatch%")),
            ("param_1".into(), serde_json::json!("%CapMatch%")),
        ],
    );
    let rows = vec![
        serde_json::json!({"id": "1", "title": "CapMatch0", "searchable_text": "CapMatch0"}),
        serde_json::json!({"id": "2", "title": "Nope", "searchable_text": "CapMatchZ"}),
        serde_json::json!({"id": "3", "title": "Other", "searchable_text": "Other"}),
    ];
    let out = apply_equality_where(rows, &compiled);
    assert_eq!(out.len(), 2);
}
