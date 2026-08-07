use crate::query::predicates::{IntPredicate, SortDirection, StringPredicate};
use crate::query::QueryCore;

#[test]
fn simple_string_equals_emits_parameterized_where() {
    let query = QueryCore::new("widget".to_string())
        .where_string(
            "name".to_string(),
            StringPredicate::Equals("alpha".to_string()),
        )
        .limit(10);
    let (sql, params) = query.to_surrealql().expect("compile");
    assert!(sql.contains("SELECT * FROM widget"));
    assert!(sql.contains("name = $param_0"));
    assert!(sql.contains("LIMIT 10"));
    assert_eq!(params.len(), 1);
    assert_eq!(params[0].1, serde_json::json!("alpha"));
}

#[test]
fn int_range_and_order_by() {
    let query = QueryCore::new("scoreboard".to_string())
        .where_int("score".to_string(), IntPredicate::GreaterThanOrEqual(10))
        .where_int("score".to_string(), IntPredicate::LessThan(100))
        .order_by("score".to_string(), SortDirection::Desc)
        .limit(5);
    let (sql, params) = query.to_surrealql().expect("compile");
    assert!(sql.contains("score >= $param_0"));
    assert!(sql.contains("score < $param_1"));
    assert!(sql.contains("ORDER BY score DESC"));
    assert!(sql.contains("LIMIT 5"));
    assert_eq!(params.len(), 2);
}

#[test]
fn projection_and_offset() {
    let query = QueryCore::new("item".to_string())
        .select(vec!["id".to_string(), "name".to_string()])
        .offset(20);
    let (sql, _) = query.to_surrealql().expect("compile");
    assert!(sql.contains("SELECT id, name FROM item"));
    assert!(sql.contains("START 20"));
}

#[test]
fn search_fields_or_contains_clause() {
    let query = QueryCore::new("article".to_string())
        .set_search_fields(vec!["title".to_string(), "body".to_string()])
        .search("rust".to_string());
    let (sql, params) = query.to_surrealql().expect("compile");
    assert!(sql.contains("title CONTAINS $param_0"));
    assert!(sql.contains("body CONTAINS $param_1"));
    assert!(sql.contains(" OR "));
    assert_eq!(params[0].1, serde_json::json!("rust"));
    assert_eq!(params[1].1, serde_json::json!("rust"));
}

#[test]
fn id_equals_uses_record_binding() {
    let query = QueryCore::new("project".to_string()).where_string(
        "id".to_string(),
        StringPredicate::Equals("project:abc".to_string()),
    );
    let (sql, params) = query.to_surrealql().expect("compile");
    assert!(sql.contains("type::record("));
    assert_eq!(params.len(), 2);
}

#[test]
fn starts_with_emits_surreal_string_fn_not_like() {
    let query = QueryCore::new("project".to_string())
        .where_string(
            "name".to_string(),
            StringPredicate::StartsWith("cx-".to_string()),
        )
        .order_by("name".to_string(), SortDirection::Desc)
        .limit(25)
        .offset(10);
    let (sql, params) = query.to_surrealql().expect("compile");
    assert!(
        sql.contains("string::starts_with(name, $param_0)"),
        "got {sql}"
    );
    assert!(!sql.to_uppercase().contains(" LIKE "));
    assert!(sql.contains("ORDER BY name DESC"));
    assert!(sql.contains("LIMIT 25"));
    assert!(sql.contains("START 10"));
    assert_eq!(params[0].1, serde_json::json!("cx-"));
}

#[test]
fn string_less_than_emits_lt_and_limit() {
    let query = QueryCore::new("catalog_ttl_probe".to_string())
        .select(vec!["id".to_string()])
        .where_string(
            crate::ttl::EXPIRE_AT_FIELD.to_string(),
            StringPredicate::LessThan("2020-01-01T00:00:00+00:00".to_string()),
        )
        .order_by(crate::ttl::EXPIRE_AT_FIELD.to_string(), SortDirection::Asc)
        .limit(32);
    let (sql, params) = query.to_surrealql().expect("compile");
    assert!(
        sql.contains(&format!("{} < $param_0", crate::ttl::EXPIRE_AT_FIELD)),
        "got {sql}"
    );
    assert!(sql.contains("LIMIT 32"), "got {sql}");
    assert_eq!(params[0].1, serde_json::json!("2020-01-01T00:00:00+00:00"));
}
