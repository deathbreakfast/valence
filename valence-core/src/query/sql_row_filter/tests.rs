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
        assert_eq!(out, Vec::<serde_json::Value>::new());
    }

    #[test]
    fn user_exists_only_on_other_team_returns_nothing() {
        let out = apply_equality_where(edges(), &team_user_query(Some("t2"), Some("uB")));
        assert_eq!(out, Vec::<serde_json::Value>::new());
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
        assert_eq!(out, Vec::<serde_json::Value>::new());
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

    fn view(id: &str, surface: &str, scope: &str, owner: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "surface": surface,
            "scope_kind": scope,
            "owner_user": {"table": "user", "id": owner},
        })
    }

    /// v1 (list, org, uA), v2 (list, org, uB), v3 (board, org, uA), v4 (list, team, uA).
    fn views() -> Vec<serde_json::Value> {
        vec![
            view("v1", "list", "org", "uA"),
            view("v2", "list", "org", "uB"),
            view("v3", "board", "org", "uA"),
            view("v4", "list", "team", "uA"),
        ]
    }

    /// The compiled shape of string predicates plus a record predicate, from the real builder.
    fn string_and_record_query(strings: &[(&str, &str)], owner: &str) -> CompiledQuery {
        let mut q = QueryCore::new("saved_view".to_string());
        for (field, value) in strings {
            q = q.where_string(
                (*field).into(),
                crate::query::StringPredicate::Equals((*value).into()),
            );
        }
        q = q.where_record(
            "owner_user".into(),
            RecordPredicate::Equals(RecordId::new("user", owner)),
        );
        let (sql, params) = q.to_sql().expect("compile");
        CompiledQuery::new(sql, params)
    }

    #[test]
    fn string_and_record_predicates_both_apply() {
        let q = string_and_record_query(&[("surface", "list")], "uA");
        assert_eq!(ids(&apply_equality_where(views(), &q)), ["v1", "v4"]);
    }

    #[test]
    fn two_string_predicates_and_a_record_predicate_all_apply() {
        let q = string_and_record_query(&[("surface", "list"), ("scope_kind", "org")], "uA");
        assert_eq!(ids(&apply_equality_where(views(), &q)), ["v1"]);
    }

    #[test]
    fn string_miss_drops_rows_the_record_predicate_matches() {
        let q = string_and_record_query(&[("surface", "calendar")], "uA");
        assert!(apply_equality_where(views(), &q).is_empty());
    }

    #[test]
    fn record_predicate_before_string_predicate_both_apply() {
        let mut q = QueryCore::new("saved_view".to_string());
        q = q.where_record(
            "owner_user".into(),
            RecordPredicate::Equals(RecordId::new("user", "uA")),
        );
        q = q.where_string(
            "surface".into(),
            crate::query::StringPredicate::Equals("board".into()),
        );
        let (sql, params) = q.to_sql().expect("compile");
        let out = apply_equality_where(views(), &CompiledQuery::new(sql, params));
        assert_eq!(ids(&out), ["v3"]);
    }

    #[test]
    fn and_only_clause_with_unsupported_operator_excludes_every_row() {
        for unsupported in [
            "a != $p1",
            "a <> $p1",
            "a IN ($p1, $p2)",
            "n BETWEEN $p1 AND $p2",
            "a = $missing",
            "a = 'unterminated",
            "(a = $p1",
        ] {
            let sql = format!("SELECT * FROM t WHERE b = $p0 AND {unsupported}");
            let params = vec![("p0", "p".into()), ("p1", "x".into()), ("p2", "y".into())];
            assert!(
                run(&sql, params).is_empty(),
                "{unsupported} must fail closed"
            );
        }
    }

    #[test]
    fn and_only_clause_does_not_confuse_param_prefixes() {
        let sql = "SELECT * FROM t WHERE a = $param_1";
        let params = vec![("param_10", "y".into()), ("param_1", "x".into())];
        assert_eq!(run(sql, params), ["r1", "r2"]);
    }
}

fn ids(rows: &[serde_json::Value]) -> Vec<&str> {
    rows.iter().map(|r| r["id"].as_str().unwrap()).collect()
}

#[test]
fn order_by_sorts_numbers_numerically() {
    let rows = vec![
        serde_json::json!({"id": "a", "created_at": 100}),
        serde_json::json!({"id": "b", "created_at": 2}),
        serde_json::json!({"id": "c", "created_at": 10}),
        serde_json::json!({"id": "d", "created_at": -5}),
        serde_json::json!({"id": "e", "created_at": 2.5}),
    ];
    let asc = apply_order_limit_offset(rows.clone(), "SELECT * FROM t ORDER BY created_at ASC");
    assert_eq!(ids(&asc), ["d", "b", "e", "c", "a"]);
    let desc = apply_order_limit_offset(rows, "SELECT * FROM t ORDER BY created_at DESC");
    assert_eq!(ids(&desc), ["a", "c", "e", "b", "d"]);
}

#[test]
fn order_by_uses_every_key_with_its_own_direction() {
    let rows = vec![
        serde_json::json!({"id": "a", "g": "x", "n": 1}),
        serde_json::json!({"id": "b", "g": "y", "n": 5}),
        serde_json::json!({"id": "c", "g": "x", "n": 9}),
        serde_json::json!({"id": "d", "g": "y", "n": 7}),
    ];
    let out = apply_order_limit_offset(rows, "SELECT * FROM t ORDER BY g ASC, n DESC");
    assert_eq!(ids(&out), ["c", "a", "d", "b"]);
}

#[test]
fn order_by_json_extract_keys_and_missing_fields() {
    let rows = vec![
        serde_json::json!({"id": "a", "body": {"rank": 3}}),
        serde_json::json!({"id": "b"}),
        serde_json::json!({"id": "c", "body": {"rank": 20}}),
    ];
    let out = apply_order_limit_offset(
        rows.clone(),
        "SELECT * FROM t ORDER BY json_extract(body, '$.rank') ASC, id ASC",
    );
    assert_eq!(ids(&out), ["b", "a", "c"]);
    let out = apply_order_limit_offset(
        rows,
        "SELECT * FROM t ORDER BY json_extract(body, '$.rank') DESC",
    );
    assert_eq!(ids(&out), ["c", "a", "b"]);
}

#[test]
fn order_by_then_offset_and_limit_window() {
    let rows: Vec<_> = (0..6)
        .map(|n| serde_json::json!({"id": format!("r{n}"), "n": n * 10}))
        .collect();
    let sql = "SELECT * FROM t ORDER BY n DESC LIMIT 2 OFFSET 1";
    assert_eq!(
        ids(&apply_order_limit_offset(rows.clone(), sql)),
        ["r4", "r3"]
    );
    let surreal = "SELECT * FROM t ORDER BY n DESC LIMIT 2 START 1";
    assert_eq!(ids(&apply_order_limit_offset(rows, surreal)), ["r4", "r3"]);
}

/// `param_0` to `param_10`, listed with `param_10` first so a lookup that stops at the first
/// name sharing a prefix with `param_1` finds the wrong value.
fn eleven_params(
    one: serde_json::Value,
    ten: serde_json::Value,
) -> Vec<(String, serde_json::Value)> {
    let mut params: Vec<_> = (0..11)
        .map(|i| {
            (
                format!("param_{i}"),
                serde_json::json!(format!("unused{i}")),
            )
        })
        .collect();
    params[1].1 = one;
    params[10].1 = ten;
    params.reverse();
    params
}

#[test]
fn equality_keeps_param_1_and_param_10_apart() {
    let compiled = CompiledQuery::new(
        "SELECT * FROM t WHERE a = $param_1 AND b = $param_10".into(),
        eleven_params(serde_json::json!("a1"), serde_json::json!("b10")),
    );
    let rows = vec![
        serde_json::json!({"id": "1", "a": "a1", "b": "b10"}),
        serde_json::json!({"id": "2", "a": "b10", "b": "a1"}),
        serde_json::json!({"id": "3", "a": "a1", "b": "other"}),
    ];
    let out = apply_equality_where(rows, &compiled);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0]["id"], "1");
}

#[test]
fn comparison_keeps_param_1_and_param_10_apart() {
    let params = eleven_params(serde_json::json!(5), serde_json::json!(50));
    let compiled = CompiledQuery::new(
        "SELECT * FROM t WHERE n >= $param_1 AND n < $param_10".into(),
        params,
    );
    let rows = (0..60)
        .map(|n| serde_json::json!({"id": n.to_string(), "n": n}))
        .collect();
    let out = apply_equality_where(rows, &compiled);
    assert_eq!(out.len(), 45);
}

#[test]
fn like_keeps_param_1_and_param_10_apart() {
    let params = eleven_params(serde_json::json!("x%"), serde_json::json!("%z"));
    let compiled = CompiledQuery::new(
        "SELECT * FROM t WHERE s LIKE $param_1 AND s LIKE $param_10".into(),
        params,
    );
    let rows = vec![
        serde_json::json!({"id": "1", "s": "xyz"}),
        serde_json::json!({"id": "2", "s": "xyy"}),
    ];
    let out = apply_equality_where(rows, &compiled);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0]["id"], "1");
}

#[test]
fn or_clause_keeps_param_1_and_param_10_apart() {
    let compiled = CompiledQuery::new(
        "SELECT * FROM t WHERE a = $param_1 OR b = $param_10".into(),
        eleven_params(serde_json::json!("a1"), serde_json::json!("b10")),
    );
    let rows = vec![
        serde_json::json!({"id": "1", "a": "a1", "b": "x"}),
        serde_json::json!({"id": "2", "a": "x", "b": "b10"}),
        serde_json::json!({"id": "3", "a": "b10", "b": "a1"}),
    ];
    let out = apply_equality_where(rows, &compiled);
    assert_eq!(out.len(), 2);
}

#[test]
fn filters_datetime_after_separates_rows_in_the_same_second() {
    let compiled = CompiledQuery::new(
        "SELECT id, body FROM typed_probe WHERE json_extract(body, '$.at') > $param_0".into(),
        vec![("param_0".into(), serde_json::json!(1_700_000_000_001_i64))],
    );
    let rows = vec![
        serde_json::json!({"id": "1", "at": 1_700_000_000_001_i64}),
        serde_json::json!({"id": "2", "at": 1_700_000_000_002_i64}),
    ];
    let out = apply_equality_where(rows, &compiled);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0]["id"], "2");
}

#[test]
fn legacy_seconds_row_sorts_before_millisecond_predicate() {
    // Predicates compare milliseconds; an unrewritten seconds row is far below the bound.
    let compiled = CompiledQuery::new(
        "SELECT id, body FROM typed_probe WHERE json_extract(body, '$.at') > $param_0".into(),
        vec![("param_0".into(), serde_json::json!(1_000_000_000_000_i64))],
    );
    let rows = vec![serde_json::json!({"id": "legacy", "at": 1_700_000_000_i64})];
    let out = apply_equality_where(rows, &compiled);
    assert_eq!(out, Vec::<serde_json::Value>::new());
}

fn like_ids(sql: &str, pattern: &str, values: &[&str]) -> Vec<String> {
    let rows = values
        .iter()
        .enumerate()
        .map(|(i, v)| serde_json::json!({"id": format!("r{i}"), "s": v}))
        .collect();
    let compiled = CompiledQuery::new(
        sql.into(),
        vec![("param_0".into(), serde_json::json!(pattern))],
    );
    apply_equality_where(rows, &compiled)
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn like_escape_contains_starts_ends_with_plain_text() {
    let values = ["core", "Hardcore!", "corex", "x"];
    let sql = "SELECT * FROM t WHERE s LIKE $param_0 ESCAPE '\\'";
    assert_eq!(like_ids(sql, "%core%", &values), ["r0", "r1", "r2"]);
    assert_eq!(like_ids(sql, "core%", &values), ["r0", "r2"]);
    assert_eq!(like_ids(sql, "%core", &values), ["r0"]);
}

#[test]
fn like_escape_makes_wildcards_and_escape_literal() {
    let sql = "SELECT * FROM t WHERE s LIKE $param_0 ESCAPE '\\'";
    let values = ["100%", "100x", "a_b", "axb", "a\\b", "ab"];
    assert_eq!(like_ids(sql, "%100\\%%", &values), ["r0"]);
    assert_eq!(like_ids(sql, "%a\\_b%", &values), ["r2"]);
    assert_eq!(like_ids(sql, "%a\\\\b%", &values), ["r4"]);
    // Unescaped, `_` and `%` stay wildcards.
    assert_eq!(like_ids(sql, "a_b", &values), ["r2", "r3", "r4"]);
}

#[test]
fn like_is_ascii_case_insensitive_like_sqlite() {
    let sql = "SELECT * FROM t WHERE s LIKE $param_0 ESCAPE '\\'";
    assert_eq!(like_ids(sql, "%CORE%", &["Core", "other"]), ["r0"]);
}

#[test]
fn not_like_and_custom_escape_char() {
    let sql = "SELECT * FROM t WHERE s NOT LIKE $param_0 ESCAPE '!'";
    assert_eq!(like_ids(sql, "%!%%", &["10%", "10"]), ["r1"]);
}

#[test]
fn malformed_escape_excludes_every_row() {
    for sql in [
        "SELECT * FROM t WHERE s LIKE $param_0 ESCAPE ''",
        "SELECT * FROM t WHERE s LIKE $param_0 ESCAPE '\\\\x'",
        "SELECT * FROM t WHERE s = $param_0 ESCAPE '\\'",
    ] {
        assert!(like_ids(sql, "%a%", &["a"]).is_empty(), "{sql}");
    }
    // A pattern that ends on a bare escape matches nothing.
    let sql = "SELECT * FROM t WHERE s LIKE $param_0 ESCAPE '\\'";
    assert_eq!(like_ids(sql, "a\\", &["a", "a\\"]), Vec::<String>::new());
}

#[test]
fn is_null_matches_absent_and_json_null_fields() {
    let rows = || {
        vec![
            serde_json::json!({"id": "absent", "body": {}}),
            serde_json::json!({"id": "null", "body": {"subject": null}}),
            serde_json::json!({"id": "set", "body": {"subject": "t:1"}}),
        ]
    };
    let ids = |sql: &str| -> Vec<String> {
        apply_equality_where(rows(), &CompiledQuery::new(sql.into(), vec![]))
            .iter()
            .map(|r| r["id"].as_str().unwrap().to_string())
            .collect()
    };
    let col = "json_extract(body, '$.subject')";
    assert_eq!(
        ids(&format!("SELECT * FROM t WHERE {col} IS NULL")),
        ["absent", "null"]
    );
    assert_eq!(
        ids(&format!("SELECT * FROM t WHERE {col} IS NOT NULL")),
        ["set"]
    );
    assert_eq!(
        ids(&format!(
            "SELECT * FROM t WHERE ({col} IS NONE OR {col} IS NULL)"
        )),
        ["absent", "null"]
    );
    assert_eq!(
        ids("SELECT * FROM t WHERE subject IS NULL AND id = 'null'"),
        ["null"]
    );
}
