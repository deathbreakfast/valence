//! Convert Surreal query/select values into JSON.

use std::collections::BTreeMap;

use surrealdb::types::{Array, Number, RecordId, SurrealValue, Value};

use valence_core::error::{Error, Result};

use super::types::SurrealAny;

pub fn surreal_any_to_json(v: &SurrealAny) -> serde_json::Value {
    match v {
        SurrealAny::Null => serde_json::Value::Null,
        SurrealAny::Bool(b) => serde_json::Value::Bool(*b),
        SurrealAny::I64(n) => serde_json::Value::Number((*n).into()),
        SurrealAny::U64(n) => serde_json::Value::Number((*n).into()),
        SurrealAny::F64(n) => serde_json::Number::from_f64(*n)
            .map_or(serde_json::Value::Null, serde_json::Value::Number),
        SurrealAny::String(s) => serde_json::Value::String(s.clone()),
        SurrealAny::Seq(items) => {
            serde_json::Value::Array(items.iter().map(surreal_any_to_json).collect())
        }
        SurrealAny::Map(m) => record_map_to_json_object(m),
    }
}

pub fn record_map_to_json_object(m: &BTreeMap<String, SurrealAny>) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    for (k, v) in m {
        obj.insert(k.clone(), surreal_any_to_json(v));
    }
    serde_json::Value::Object(obj)
}

pub fn map_looks_like_surreal_thing_only(m: &BTreeMap<String, SurrealAny>) -> bool {
    if m.len() != 2 {
        return false;
    }
    m.contains_key("tb") && m.contains_key("id")
}

pub fn thing_to_id_only(mut s: String) -> String {
    s = s.split(':').next_back().unwrap_or(&s).to_string();
    s = s
        .replace(['⟩', '⟨', '›', '‹', '»', '«'], "")
        .trim()
        .to_string();
    s
}

/// `tb` + `id` map from Surreal → record id string for [`select_record_json`].
pub fn thing_only_key_from_tb_id_map(m: &BTreeMap<String, SurrealAny>) -> Result<String> {
    let tb = m
        .get("tb")
        .and_then(|v| match v {
            SurrealAny::String(s) => Some(s.as_str()),
            _ => None,
        })
        .ok_or_else(|| Error::Validation("Malformed record id (tb)".into()))?;
    let id_part = m
        .get("id")
        .ok_or_else(|| Error::Validation("Malformed record id (id)".into()))?;
    let id_str = match id_part {
        SurrealAny::String(s) => s.clone(),
        SurrealAny::I64(n) => n.to_string(),
        SurrealAny::U64(n) => n.to_string(),
        _ => {
            return Err(Error::Validation("Malformed record id (id type)".into()));
        }
    };
    Ok(thing_to_id_only(format!("{tb}:{id_str}")))
}

fn surreal_any_from_number(n: Number) -> SurrealAny {
    match Value::Number(n).into_json_value() {
        serde_json::Value::Number(num) => {
            if let Some(i) = num.as_i64() {
                SurrealAny::I64(i)
            } else if let Some(u) = num.as_u64() {
                SurrealAny::U64(u)
            } else if let Some(f) = num.as_f64() {
                SurrealAny::F64(f)
            } else {
                SurrealAny::Null
            }
        }
        _ => SurrealAny::Null,
    }
}

fn surreal_any_from_array(arr: Array) -> SurrealAny {
    SurrealAny::Seq(
        arr.into_inner()
            .into_iter()
            .map(value_to_surreal_any)
            .collect(),
    )
}

fn surreal_any_from_object(obj: surrealdb::types::Object) -> SurrealAny {
    let mut out = BTreeMap::new();
    for (k, val) in obj.into_inner() {
        out.insert(k, value_to_surreal_any(val));
    }
    SurrealAny::Map(out)
}

fn surreal_any_from_record_id(record: RecordId) -> SurrealAny {
    let wire = match Value::RecordId(record).into_json_value() {
        serde_json::Value::String(s) => s,
        other => other.to_string(),
    };
    SurrealAny::String(wire)
}

fn surreal_any_from_json_item(item: &serde_json::Value) -> SurrealAny {
    serde_json::from_value(item.clone()).unwrap_or(SurrealAny::Null)
}

fn surreal_any_from_json_value(json: &serde_json::Value) -> SurrealAny {
    if json.is_null() {
        return SurrealAny::Null;
    }
    if let Some(b) = json.as_bool() {
        return SurrealAny::Bool(b);
    }
    if let Some(n) = json.as_i64() {
        return SurrealAny::I64(n);
    }
    if let Some(n) = json.as_u64() {
        return SurrealAny::U64(n);
    }
    if let Some(n) = json.as_f64() {
        return SurrealAny::F64(n);
    }
    if let Some(s) = json.as_str() {
        return SurrealAny::String(s.to_string());
    }
    if let Some(arr) = json.as_array() {
        return SurrealAny::Seq(arr.iter().map(surreal_any_from_json_item).collect());
    }
    if let Some(map) = json.as_object() {
        let mut out = BTreeMap::new();
        for (k, item) in map {
            out.insert(k.clone(), surreal_any_from_json_item(item));
        }
        return SurrealAny::Map(out);
    }
    SurrealAny::String(json.to_string())
}

fn surreal_any_from_fallback(other: Value) -> SurrealAny {
    surreal_any_from_json_value(&other.into_json_value())
}

pub fn value_to_surreal_any(v: Value) -> SurrealAny {
    match v {
        Value::None | Value::Null => SurrealAny::Null,
        Value::Bool(b) => SurrealAny::Bool(b),
        Value::Number(n) => surreal_any_from_number(n),
        Value::String(s) => SurrealAny::String(s),
        Value::Array(arr) => surreal_any_from_array(arr),
        Value::Object(obj) => surreal_any_from_object(obj),
        Value::RecordId(record) => surreal_any_from_record_id(record),
        other => surreal_any_from_fallback(other),
    }
}

pub fn try_value_as_record_map(v: &Value) -> Option<BTreeMap<String, SurrealAny>> {
    match v {
        Value::Object(obj) => {
            let mut out = BTreeMap::new();
            for (k, val) in obj.clone().into_inner() {
                out.insert(k, value_to_surreal_any(val));
            }
            Some(out)
        }
        _ => None,
    }
}

pub fn try_value_as_record_id(v: &Value) -> Option<RecordId> {
    match v {
        Value::RecordId(record) => Some(record.clone()),
        other => RecordId::from_value(other.clone()).ok(),
    }
}

fn query_result_first_row(result: Value) -> Option<Value> {
    match result {
        Value::Array(arr) => arr.into_inner().into_iter().next(),
        Value::None | Value::Null => None,
        other => Some(other),
    }
}

fn query_result_rows(result: Value) -> Vec<Value> {
    match result {
        Value::Array(arr) => arr.into_inner(),
        Value::None | Value::Null => vec![],
        other => vec![other],
    }
}

async fn fetch_first_row_select_rid<C>(
    db: &surrealdb::Surreal<C>,
    table: &str,
    id: &str,
) -> Result<Option<Value>>
where
    C: surrealdb::Connection,
{
    let rid = RecordId::new(table, id);
    let mut response = db
        .query("SELECT * FROM $rid")
        .bind(("rid", rid))
        .await
        .map_err(crate::error::db_err)?;
    let result: Value = response.take(0).map_err(crate::error::db_err)?;
    Ok(query_result_first_row(result))
}

async fn record_row_value_to_json_optional<C>(
    db: &surrealdb::Surreal<C>,
    table: &str,
    id: &str,
    mut row: Value,
    start_depth: u8,
) -> Result<Option<serde_json::Value>>
where
    C: surrealdb::Connection,
{
    use super::types::MAX_QUERY_ROW_FETCH_DEPTH;

    let mut depth = start_depth;

    loop {
        if depth >= MAX_QUERY_ROW_FETCH_DEPTH {
            return Err(Error::Validation(
                "Surreal query row decode exceeded max depth".into(),
            ));
        }

        if let Some(m) = try_value_as_record_map(&row) {
            if map_looks_like_surreal_thing_only(&m) {
                depth += 1;
                let Some(next) = fetch_first_row_select_rid(db, table, id).await? else {
                    return Ok(None);
                };
                row = next;
                continue;
            }
            let mut json = record_map_to_json_object(&m);
            valence_core::row_json::normalize_record_id_field(table, &mut json);
            return Ok(Some(json));
        }

        if try_value_as_record_id(&row).is_some() {
            depth += 1;
            let Some(next) = fetch_first_row_select_rid(db, table, id).await? else {
                return Ok(None);
            };
            row = next;
            continue;
        }

        return Err(Error::Validation(
            "Failed to decode query row as record map or record id".into(),
        ));
    }
}

/// Decode each row from a `db.query` statement for arbitrary compiled queries (unknown tables).
pub async fn decode_query_response_rows_to_json<C>(
    db: &surrealdb::Surreal<C>,
    result: Value,
) -> Result<Vec<serde_json::Value>>
where
    C: surrealdb::Connection,
{
    let mut out = Vec::new();
    for row in query_result_rows(result) {
        out.push(query_row_value_to_json(db, row, 0).await?);
    }
    Ok(out)
}

async fn query_row_value_to_json<C>(
    db: &surrealdb::Surreal<C>,
    row: Value,
    depth: u8,
) -> Result<serde_json::Value>
where
    C: surrealdb::Connection,
{
    if let Some(m) = try_value_as_record_map(&row) {
        if map_looks_like_surreal_thing_only(&m) {
            let tb = m
                .get("tb")
                .and_then(|v| match v {
                    SurrealAny::String(s) => Some(s.as_str()),
                    _ => None,
                })
                .ok_or_else(|| Error::Validation("Malformed record id (tb)".into()))?;
            let id = thing_only_key_from_tb_id_map(&m)?;
            return record_row_value_to_json_optional(db, tb, &id, row, depth)
                .await?
                .ok_or_else(|| {
                    Error::Validation(
                        "Expected record after record-id-shaped query row, got empty".into(),
                    )
                });
        }
        return Ok(record_map_to_json_object(&m));
    }

    if let Some(record) = try_value_as_record_id(&row) {
        let table = record.table.to_string();
        let id = crate::record_id::extract_id_from_surreal_record(&record)?;
        return record_row_value_to_json_optional(db, &table, &id, row, depth)
            .await?
            .ok_or_else(|| {
                Error::Validation("Expected record after record-id query row, got empty".into())
            });
    }

    Err(Error::Validation(
        "Failed to decode query result row".into(),
    ))
}

async fn select_record_json_via_query<C>(
    db: &surrealdb::Surreal<C>,
    table: &str,
    id: &str,
) -> Result<Option<serde_json::Value>>
where
    C: surrealdb::Connection,
{
    let rid = RecordId::new(table, id);
    let mut response = db
        .query("SELECT * FROM $rid")
        .bind(("rid", rid))
        .await
        .map_err(crate::error::db_err)?;
    let result: Value = response.take(0).map_err(crate::error::db_err)?;
    let Some(row) = query_result_first_row(result) else {
        return Ok(None);
    };
    record_row_value_to_json_optional(db, table, id, row, 0).await
}

/// Load a full record JSON object via `SELECT * FROM $rid` when [`db.select`] returned a bare id.
pub async fn select_record_json<C>(
    db: &surrealdb::Surreal<C>,
    table: &str,
    id: &str,
) -> Result<Option<serde_json::Value>>
where
    C: surrealdb::Connection,
{
    select_record_json_via_query(db, table, id).await
}

/// Decode a single `db.select` response into JSON, refetching bare record ids when needed.
#[allow(dead_code)]
pub async fn decode_select_response_to_json<C>(
    db: &surrealdb::Surreal<C>,
    table: &str,
    id: &str,
    raw: Value,
) -> Result<Option<serde_json::Value>>
where
    C: surrealdb::Connection,
{
    if let Some(m) = try_value_as_record_map(&raw) {
        if !map_looks_like_surreal_thing_only(&m) {
            return Ok(Some(record_map_to_json_object(&m)));
        }
    }

    if try_value_as_record_id(&raw).is_some() || matches!(raw, Value::RecordId(_)) {
        return select_record_json_via_query(db, table, id).await;
    }

    if let Value::Object(ref obj) = raw {
        if obj.is_empty() {
            return Ok(None);
        }
    }

    Ok(Some(raw.into_json_value()))
}

/// `db.select` on a record resource; returns JSON or refetches when the client returns a bare id.
#[allow(dead_code)]
pub async fn select_record_resource_json<C>(
    db: &surrealdb::Surreal<C>,
    table: &str,
    id: &str,
) -> Result<Option<serde_json::Value>>
where
    C: surrealdb::Connection,
{
    let resource = RecordId::new(table, id);
    let raw = db.select(resource).await.map_err(crate::error::db_err)?;
    match raw {
        Some(value) => decode_select_response_to_json(db, table, id, value).await,
        None => Ok(None),
    }
}
