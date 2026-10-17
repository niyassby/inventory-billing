//! Reads v1 tables generically into JSON-like rows. v1 grew columns over time (description,
//! image_path, tax_id, unit_id, offer_price…), so a missing column must read as "absent", not as
//! an error.

use rusqlite::types::ValueRef;
use rusqlite::Connection;
use serde_json::{Map, Number, Value};

#[derive(Debug, Clone)]
pub struct LegacyRow(pub Map<String, Value>);

impl LegacyRow {
    /// Text value; empty or whitespace-only strings count as absent.
    pub fn text(&self, key: &str) -> Option<String> {
        match self.0.get(key)? {
            Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        }
    }

    pub fn text_or(&self, key: &str, fallback: &str) -> String {
        self.text(key).unwrap_or_else(|| fallback.to_string())
    }

    pub fn id(&self) -> String {
        self.text("id").unwrap_or_default()
    }

    pub fn real(&self, key: &str) -> f64 {
        match self.0.get(key) {
            Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
            Some(Value::String(s)) => s.trim().parse().unwrap_or(0.0),
            _ => 0.0,
        }
    }

    pub fn flag(&self, key: &str) -> bool {
        self.real(key) != 0.0
    }

    /// The row as JSON for `legacy_records`, minus the listed secret columns.
    pub fn to_json_without(&self, secret_columns: &[&str]) -> String {
        let mut map = self.0.clone();
        for column in secret_columns {
            map.remove(*column);
        }
        Value::Object(map).to_string()
    }
}

pub fn table_exists(conn: &Connection, table: &str) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |r| r.get::<_, i64>(0),
    )
    .map(|n| n > 0)
}

/// All rows of a v1 table (empty if the table doesn't exist in this v1 version).
/// `table` is always a compile-time constant from the migration code, never user input.
pub fn read_table(conn: &Connection, table: &str) -> rusqlite::Result<Vec<LegacyRow>> {
    if !table_exists(conn, table)? {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(&format!("SELECT * FROM \"{table}\" ORDER BY rowid"))?;
    let columns: Vec<String> = stmt
        .column_names()
        .iter()
        .map(|c| (*c).to_string())
        .collect();
    let rows = stmt.query_map([], |row| {
        let mut map = Map::new();
        for (index, name) in columns.iter().enumerate() {
            let value = match row.get_ref(index)? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(i) => Value::Number(i.into()),
                ValueRef::Real(f) => Number::from_f64(f).map_or(Value::Null, Value::Number),
                ValueRef::Text(t) => Value::String(String::from_utf8_lossy(t).into_owned()),
                ValueRef::Blob(b) => Value::String(hex::encode(b)),
            };
            map.insert(name.clone(), value);
        }
        Ok(LegacyRow(map))
    })?;
    rows.collect()
}
