//! Windows 11's index, `Windows.db`: a SQLite database. Items are rows of
//! `SystemIndex_1_PropertyStore` (`WorkId`, `ColumnId`, `Value`), one per
//! property an item has; `SystemIndex_1_PropertyStore_Metadata` names each
//! `ColumnId` (`System.ItemPathDisplay`) and gives its type. Integers and
//! text are SQLite values; times, 64-bit numbers and GUIDs are
//! little-endian bytes; lists of strings are UTF-16, separated by NULs.
//! There are no gatherer tables.

use std::collections::HashMap;

use sqlite::Database;

use crate::item::Item;
use crate::property::{Decoder, Types, VT_BLOB, VT_BOOL, VT_LPWSTR, VT_VECTOR};
use crate::{Error, Index, Layout};

/// The table of items' properties.
const PROPERTIES: &str = "SystemIndex_1_PropertyStore";
/// The table naming each property column.
const METADATA: &str = "SystemIndex_1_PropertyStore_Metadata";

/// A property column: its name as the ESE layouts write it
/// (`System_ItemPathDisplay`) and its type.
struct Column {
    name: String,
    vt: u32,
}

/// Read `Windows.db`, with its write-ahead log (`wal`, may be empty).
pub(crate) fn read(database: &[u8], wal: &[u8]) -> Result<Index, Error> {
    let db = Database::open_with_wal(database, wal).map_err(|e| Error(e.to_string()))?;
    let (Some(metadata), Some(_)) = (db.table(METADATA), db.table(PROPERTIES)) else {
        return Err(Error(format!(
            "no {PROPERTIES} or {METADATA}: not a Windows Search index"
        )));
    };
    let mut problems = db.problems.clone();
    let columns = columns(&db, metadata, &mut problems);
    let decoder = Decoder {
        layout: Layout::Sqlite,
        types: Types::with(
            columns
                .values()
                .map(|column| (column.name.clone(), list_type(column))),
        ),
    };
    let items = items(&db, &columns, &decoder, &mut problems);
    Ok(Index {
        table: Some(PROPERTIES.to_owned()),
        layout: Some(Layout::Sqlite),
        items,
        gathered: Vec::new(),
        problems,
    })
}

/// The property columns `metadata` lists, by `ColumnId`.
fn columns(
    db: &Database<'_>,
    metadata: &sqlite::Table,
    problems: &mut Vec<String>,
) -> HashMap<i64, Column> {
    let mut columns = HashMap::new();
    let Ok(mut rows) = db.rows(&metadata.name) else {
        return columns;
    };
    let at = |name| metadata.columns.iter().position(|c| c.name == name);
    let (name_at, vt_at) = (at("Name"), at("VariantType"));
    for row in &mut rows {
        let value = |at: Option<usize>| at.and_then(|at| row.values.get(at));
        let name = value(name_at).and_then(sqlite::Value::as_text);
        let vt = value(vt_at)
            .and_then(sqlite::Value::as_integer)
            .and_then(|vt| u32::try_from(vt).ok());
        if let (Some(name), Some(vt)) = (name, vt) {
            let name = name.replace('.', "_");
            columns.insert(row.rowid, Column { name, vt });
        }
    }
    problems.extend(rows.problems().iter().map(|p| format!("{METADATA}: {p}")));
    columns
}

/// A Windows property's `VT_BLOB` holds strings, UTF-16 and separated by
/// NULs (`System.Kind`, `System.Activity.AppIdList`); the index's own
/// (`InvertedOnlyMD5`) are bytes.
fn list_type(column: &Column) -> u32 {
    if column.vt == VT_BLOB && column.name.starts_with("System_") {
        VT_VECTOR | VT_LPWSTR
    } else {
        column.vt
    }
}

/// The items, one per `WorkId`, in `WorkId` order (the table's key).
fn items(
    db: &Database<'_>,
    columns: &HashMap<i64, Column>,
    decoder: &Decoder,
    problems: &mut Vec<String>,
) -> Vec<Item> {
    let Ok(mut entries) = db.index_entries(PROPERTIES) else {
        return Vec::new();
    };
    let mut items = Vec::new();
    let mut current: Option<(i64, Vec<_>)> = None;
    for entry in &mut entries {
        let [work_id, column_id, value, ..] = entry.values.as_slice() else {
            problems.push(format!(
                "{PROPERTIES}: a row of {} values",
                entry.values.len()
            ));
            continue;
        };
        let (Some(work_id), Some(column_id)) = (work_id.as_integer(), column_id.as_integer())
        else {
            problems.push(format!(
                "{PROPERTIES}: a row without its WorkId or ColumnId"
            ));
            continue;
        };
        let Some(column) = columns.get(&column_id) else {
            problems.push(format!(
                "{PROPERTIES}, item {work_id}: column {column_id} not in {METADATA}"
            ));
            continue;
        };
        if current.as_ref().is_some_and(|(id, _)| *id != work_id) {
            let (id, properties) = current.take().expect("checked");
            items.push(Item::from_properties(Some(id), properties));
        }
        let properties = &mut current.get_or_insert_with(|| (work_id, Vec::new())).1;
        let value = ese_value(value, column.vt);
        let Some((property, problem)) = decoder.decode(&column.name, &value) else {
            continue;
        };
        if let Some(problem) = problem {
            problems.push(format!(
                "{PROPERTIES}, item {work_id}, {}: {problem}",
                column.name
            ));
        }
        properties.push((column.name.clone(), property));
    }
    if let Some((id, properties)) = current {
        items.push(Item::from_properties(Some(id), properties));
    }
    problems.extend(
        entries
            .problems()
            .iter()
            .map(|p| format!("{PROPERTIES}: {p}")),
    );
    items
}

/// A SQLite value as the decoder takes it; integers of `VT_BOOL`
/// properties are booleans.
fn ese_value(value: &sqlite::Value, vt: u32) -> ese::Value {
    match value {
        sqlite::Value::Null => ese::Value::Null,
        sqlite::Value::Integer(value) if vt == VT_BOOL => ese::Value::Bool(*value != 0),
        sqlite::Value::Integer(value) => ese::Value::I64(*value),
        sqlite::Value::Real(value) => ese::Value::F64(*value),
        sqlite::Value::Text(text) => ese::Value::Text(text.clone()),
        sqlite::Value::Blob(bytes) => ese::Value::Binary(bytes.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_properties_blobs_are_strings() {
        let kind = Column {
            name: "System_Kind".to_owned(),
            vt: VT_BLOB,
        };
        assert_eq!(list_type(&kind), VT_VECTOR | VT_LPWSTR);
        let md5 = Column {
            name: "InvertedOnlyMD5".to_owned(),
            vt: VT_BLOB,
        };
        assert_eq!(list_type(&md5), VT_BLOB);
    }

    #[test]
    fn booleans_follow_the_type() {
        let one = sqlite::Value::Integer(1);
        assert_eq!(ese_value(&one, VT_BOOL), ese::Value::Bool(true));
        assert_eq!(ese_value(&one, 3), ese::Value::I64(1));
    }

    #[test]
    fn other_databases_are_errors() {
        assert!(read(b"not a database", &[]).is_err());
    }
}
