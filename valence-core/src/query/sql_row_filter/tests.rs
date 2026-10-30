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

#[allow(clippy::assert_is_empty)] // empty-result checks read better as assert!(rows.is_empty())
mod clause_matching {
    use super::*;
    use crate::query::sql_row_filter::clause::split_top_level;
    use crate::query::{QueryCore, RecordPredicate};
    use crate::RecordId;

    fn ids(rows: &[serde_json::Value]) -> Vec<&str> {
        rows.iter().map(|r| r["id"].as_str().unwrap()).collect()
    }

    fn edge(id: &str, team: &str, user: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "team_id": {"table": "team", "id": team},
            "user_id": {"table": "user", "id": user},
        })
    }

    /// e1 (t1,uA), e2 (t1,uB), e3 (t2,uA).
    fn edges() -> Vec<serde_json::Value> {
        vec![
            edge("e1", "t1", "uA"),
            edge("e2", "t1", "uB"),
            edge("e3", "t2", "uA"),
        ]
    }

    /// The compiled shape of `where_team_id(Equals).where_user_id(Equals)`, from the real
    /// query builder, so this breaks if the emitted SQL changes.
    fn team_user_query(team: Option<&str>, user: Option<&str>) -> CompiledQuery {
        let mut q = QueryCore::new("team_member".to_string());
        if let Some(t) = team {
            q = q.where_record(
                "team_id".into(),
                RecordPredicate::Equals(RecordId::new("team", t)),
            );
        }
        if let Some(u) = user {
            q = q.where_record(
                "user_id".into(),
                RecordPredicate::Equals(RecordId::new("user", u)),
            );
        }
        let (sql, params) = q.to_sql().expect("compile");
        CompiledQuery::new(sql, params)
    }

    fn run(sql: &str, params: Vec<(&str, serde_json::Value)>) -> Vec<String> {
        let params = params
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        let rows = vec![
            serde_json::json!({"id": "r1", "a": "x", "b": "p", "n": 5, "t": "alpha"}),
            serde_json::json!({"id": "r2", "a": "x", "b": "q", "n": 9, "t": "beta"}),
            serde_json::json!({"id": "r3", "a": "y", "b": "p", "n": 9, "t": "alpha"}),
        ];
        let out = apply_equality_where(rows, &CompiledQuery::new(sql.into(), params));
        out.iter()
            .map(|r| r["id"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn team_and_user_record_predicates_both_apply() {
        let out = apply_equality_where(edges(), &team_user_query(Some("t1"), Some("uA")));
        assert_eq!(ids(&out), ["e1"]);
    }

    #[test]
    fn team_matches_but_user_absent_returns_nothing() {
        let out = apply_equality_where(edges(), &team_user_query(Some("t1"), Some("uC")));
        assert!(out.is_empty(), "got {:?}", ids(&out));
    }

    #[test]
    fn user_matches_but_team_absent_returns_nothing() {
        let out = apply_equality_where(edges(), &team_user_query(Some("t9"), Some("uA")));
        assert!(out.is_empty(), "got {:?}", ids(&out));
    }

    #[test]
    fn neither_team_nor_user_matches_returns_nothing() {
        let out = apply_equality_where(edges(), &team_user_query(Some("t9"), Some("uC")));
        assert!(out.is_empty());
    }

    #[test]
    fn user_exists_only_on_other_team_returns_nothing() {
        let out = apply_equality_where(edges(), &team_user_query(Some("t2"), Some("uB")));
        assert!(out.is_empty());
    }

    #[test]
    fn team_only_query_returns_every_edge_of_the_team() {
        let out = apply_equality_where(edges(), &team_user_query(Some("t1"), None));
        assert_eq!(ids(&out), ["e1", "e2"]);
    }

    #[test]
    fn three_conjuncts_with_one_failing_return_nothing() {
        let sql = "SELECT * FROM t WHERE (a = $p0 OR a = $p1) AND (b = $p2 OR b = $p3) AND (t = $p4 OR t = $p5)";
        let params = |last: &str| {
            vec![
                ("p0", "x".into()),
                ("p1", "zz".into()),
                ("p2", "p".into()),
                ("p3", "zz".into()),
                ("p4", last.into()),
                ("p5", "zz".into()),
            ]
        };
        assert_eq!(run(sql, params("alpha")), ["r1"]);
        assert!(run(sql, params("gamma")).is_empty());
    }

    #[test]
    fn two_or_groups_need_both_groups_to_match() {
        let sql = "SELECT * FROM t WHERE (a = $p0 OR a = $p1) AND (b = $p2 OR b = $p3)";
        // r2 is in the first group only, r3 in the second only.
        let out = run(
            sql,
            vec![
                ("p0", "x".into()),
                ("p1", "zz".into()),
                ("p2", "p".into()),
                ("p3", "p".into()),
            ],
        );
        assert_eq!(out, ["r1"]);
    }

    #[test]
    fn and_with_like_conjunct() {
        let sql = "SELECT * FROM t WHERE (a = $p0 OR a = $p1) AND t LIKE $p2";
        let params = vec![("p0", "x".into()), ("p1", "y".into()), ("p2", "al%".into())];
        assert_eq!(run(sql, params), ["r1", "r3"]);
        let none = vec![("p0", "x".into()), ("p1", "y".into()), ("p2", "zz%".into())];
        assert!(run(sql, none).is_empty());
    }

    #[test]
    fn lowercase_keywords_behave_like_uppercase() {
        let sql = "SELECT * FROM t WHERE (a = $p0 or a = $p1) and (b = $p2 or b = $p3)";
        let out = run(
            sql,
            vec![
                ("p0", "x".into()),
                ("p1", "zz".into()),
                ("p2", "p".into()),
                ("p3", "zz".into()),
            ],
        );
        assert_eq!(out, ["r1"]);
    }

    #[test]
    fn and_inside_a_quoted_literal_does_not_split() {
        assert_eq!(
            split_top_level("(a = 'x AND y' OR b = $p) AND c = $q", " AND ").unwrap(),
            ["(a = 'x AND y' OR b = $p)", "c = $q"]
        );
        assert_eq!(
            split_top_level("a = 'p AND q'", " AND ").unwrap(),
            ["a = 'p AND q'"]
        );
    }

    #[test]
    fn and_inside_json_extract_or_cast_parens_does_not_split() {
        let clause = "CAST(json_extract(price, '$.a AND b') AS INTEGER) = $p0 AND x = $p1";
        assert_eq!(
            split_top_level(clause, " AND ").unwrap(),
            [
                "CAST(json_extract(price, '$.a AND b') AS INTEGER) = $p0",
                "x = $p1"
            ]
        );
        assert_eq!(
            split_top_level("f(a AND b) = $p", " AND ").unwrap().len(),
            1
        );
    }

    #[test]
    fn missing_param_key_matches_nothing_without_panicking() {
        let sql = "SELECT * FROM t WHERE (a = $p0 OR a = $p1) AND (b = $p2 OR b = $p3)";
        // `p3` is named in the clause but absent from params; `p2` still matches nothing useful.
        let out = run(
            sql,
            vec![("p0", "x".into()), ("p1", "y".into()), ("p2", "zz".into())],
        );
        assert!(out.is_empty());
        assert!(run(sql, vec![]).is_empty());
    }

    #[test]
    fn unbalanced_parens_match_nothing_without_panicking() {
        assert!(split_top_level("(a = $p0 OR a = $p1 AND b = $p2", " AND ").is_none());
        assert!(split_top_level("a = $p0) AND (b = $p2", " AND ").is_none());
        let sql = "SELECT * FROM t WHERE (a = $p0 OR a = $p1 AND b = $p2";
        assert!(run(
            sql,
            vec![("p0", "x".into()), ("p1", "y".into()), ("p2", "p".into())]
        )
        .is_empty());
    }

    #[test]
    fn unbalanced_quote_matches_nothing_without_panicking() {
        let sql = "SELECT * FROM t WHERE (a = $p0 OR a = 'x) AND b = $p2";
        assert!(run(sql, vec![("p0", "x".into()), ("p2", "p".into())]).is_empty());
    }

    #[test]
    fn range_conjunct_with_or_group_requires_both() {
        let sql = "SELECT * FROM t WHERE n >= $p0 AND (a = $p1 OR a = $p2)";
        // n >= 9 keeps r2, r3; a in {x} keeps r1, r2; both: r2.
        let out = run(
            sql,
            vec![("p0", 9.into()), ("p1", "x".into()), ("p2", "zz".into())],
        );
        assert_eq!(out, ["r2"]);
    }

    #[test]
    fn unrecognised_conjunct_excludes_every_row() {
        let is_null = "SELECT * FROM t WHERE a IS NULL AND (b = $p0 OR b = $p1)";
        assert!(run(is_null, vec![("p0", "p".into()), ("p1", "q".into())]).is_empty());
        let not_equal = "SELECT * FROM t WHERE a != $p2 AND (b = $p0 OR b = $p1)";
        assert!(run(
            not_equal,
            vec![("p0", "p".into()), ("p1", "q".into()), ("p2", "x".into())]
        )
        .is_empty());
    }

    #[test]
    fn or_only_clause_still_matches_any_term() {
        // An OR-only clause keeps a row when any term matches.
        let sql = "SELECT * FROM t WHERE a = $p0 OR b = $p1";
        assert_eq!(
            run(sql, vec![("p0", "y".into()), ("p1", "q".into())]),
            ["r2", "r3"]
        );
    }

    #[test]
    fn quoted_literal_next_to_or_group_matches_like_a_param() {
        // The ownership pending-deletion lookup has this shape.
        let literal = "SELECT * FROM t WHERE a = $p0 AND b = 'p' AND (t = $p1 OR t = $p2)";
        let param = "SELECT * FROM t WHERE a = $p0 AND b = $p3 AND (t = $p1 OR t = $p2)";
        let params = || {
            vec![
                ("p0", "x".into()),
                ("p1", "alpha".into()),
                ("p2", "gamma".into()),
                ("p3", "p".into()),
            ]
        };
        assert_eq!(run(literal, params()), ["r1"]);
        assert_eq!(run(literal, params()), run(param, params()));
    }

    #[test]
    fn quoted_literal_that_does_not_match_drops_the_row() {
        let sql = "SELECT * FROM t WHERE b = 'zz' AND (a = $p0 OR a = $p1)";
        assert!(run(sql, vec![("p0", "x".into()), ("p1", "y".into())]).is_empty());
    }

    #[test]
    fn numeric_literal_next_to_or_group() {
        let sql = "SELECT * FROM t WHERE n >= 9 AND (a = $p0 OR a = $p1)";
        assert_eq!(
            run(sql, vec![("p0", "x".into()), ("p1", "zz".into())]),
            ["r2"]
        );
        let eq = "SELECT * FROM t WHERE n = 5 OR b = $p0";
        assert_eq!(run(eq, vec![("p0", "q".into())]), ["r1", "r2"]);
    }

    #[test]
    fn quoted_literal_with_escaped_quote_or_keywords_inside() {
        let rows = vec![
            serde_json::json!({"id": "r1", "a": "it's", "b": "p AND q"}),
            serde_json::json!({"id": "r2", "a": "its", "b": "p"}),
        ];
        let compiled = CompiledQuery::new(
            "SELECT * FROM t WHERE (a = 'it''s' OR a = $p0) AND b = 'p AND q'".into(),
            vec![("p0".to_string(), "none".into())],
        );
        assert_eq!(ids(&apply_equality_where(rows, &compiled)), ["r1"]);
    }

    #[test]
    fn unquoted_non_numeric_rhs_still_excludes_every_row() {
        let sql = "SELECT * FROM t WHERE a = b AND (b = $p0 OR b = $p1)";
        assert!(run(sql, vec![("p0", "p".into()), ("p1", "q".into())]).is_empty());
        let null = "SELECT * FROM t WHERE a = NULL OR b = $p0";
        assert_eq!(run(null, vec![("p0", "q".into())]), ["r2"]);
    }
}
