//! The Windows Search index (`Windows.edb`, in
//! `ProgramData\Microsoft\Search\Data\Applications\Windows`): what the
//! indexer knew of every file, folder, e-mail and page of browser history
//! it indexed. Paths, sizes, times, owners, the first words of documents,
//! kept after the files themselves were deleted.
//!
//! The database is an ESE file, read with `sootmark-ese`. Its property
//! table has one row per item, keyed by its WorkID, and a column per
//! Windows property: `SystemIndex_PropertyStore` from Windows 10 (columns
//! named with the property's number, `4447-System_ItemPathDisplay`),
//! `SystemIndex_0A` in Windows Vista and 7 (`System_ItemPathDisplay`). The
//! gatherer's tables, `SystemIndex_Gthr` and `SystemIndex_GthrPth`, list
//! the items it found and the folders they are in.
//!
//! Windows 11 keeps the same properties in a SQLite database, `Windows.db`
//! (read with `sootmark-sqlite`, with its write-ahead log when given one):
//! one row per property of an item, the properties named in a table of
//! their own; no gatherer tables.
//!
//! ```no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let index = search::read(&std::fs::read("Windows.edb")?)?;
//! for item in &index.items {
//!     println!("{:?} {:?} {:?} {:?}", item.path, item.size, item.modified, item.summary);
//! }
//! for found in index.gathered.iter().filter(|g| !g.indexed) {
//!     println!("found, not indexed: {:?}", found.path());
//! }
//! # Ok(())
//! # }
//! ```
//!
//! Damage is reported in `problems`, never a panic; a value this crate
//! can't decode is kept as bytes.

mod encoded;
mod gather;
mod item;
mod property;
mod windows_db;

use std::collections::HashSet;

use ese::{Database, Table};

pub use gather::Gathered;
pub use item::Item;
pub use property::Property;

use item::ID_COLUMNS;
use property::{property_name, Decoder, Types};

/// This crate's version, for records of what parsed them.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The first bytes of a SQLite database.
const SQLITE_SIGNATURE: &[u8] = b"SQLite format 3\0";
/// The properties one of which marks the property table.
const ITEM_PROPERTIES: [&str; 2] = ["System_ItemPathDisplay", "System_ItemUrl"];

/// How the index is laid out, by Windows version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// `SystemIndex_0A` (Windows Vista and 7): columns named after
    /// properties alone; numbers big-endian; the text summary compressed
    /// and obfuscated by Windows Search.
    Legacy,
    /// `SystemIndex_PropertyStore` (Windows 10): columns named with the
    /// property's number first; numbers little-endian.
    PropertyStore,
    /// `Windows.db` (Windows 11): SQLite, a row per property of an item;
    /// numbers little-endian.
    Sqlite,
}

/// Which database a Windows Search index is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// `Windows.edb`: ESE (Windows Vista to 10).
    Ese,
    /// `Windows.db`: SQLite (Windows 11).
    Sqlite,
}

/// A Windows Search index.
#[derive(Debug, Clone, PartialEq)]
pub struct Index {
    /// The property table read, `None` when the database has none.
    pub table: Option<String>,
    /// Its layout.
    pub layout: Option<Layout>,
    /// The items it holds, in WorkID order.
    pub items: Vec<Item>,
    /// The gatherer's items, with their rebuilt paths.
    pub gathered: Vec<Gathered>,
    /// Damage in the database, and values that don't decode.
    pub problems: Vec<String>,
}

/// Why a file can't be read as a Windows Search index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// Which index a file named `name` (a path or a bare name) is:
/// `Windows.edb` or `Windows.db`, case ignored.
#[must_use]
pub fn detect(name: &str) -> Option<Format> {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    if base.eq_ignore_ascii_case("Windows.edb") {
        Some(Format::Ese)
    } else if base.eq_ignore_ascii_case("Windows.db") {
        Some(Format::Sqlite)
    } else {
        None
    }
}

/// Read a Windows Search index, `Windows.edb` or `Windows.db` (told apart
/// by their contents).
///
/// # Errors
/// As [`read_with_log`].
pub fn read(data: &[u8]) -> Result<Index, Error> {
    read_with_log(data, &[])
}

/// Read a Windows Search index with, for `Windows.db`, its write-ahead log
/// (the `-wal` file beside it; may be empty, and is ignored for
/// `Windows.edb`).
///
/// # Errors
/// When it is neither an ESE nor a SQLite database, or has neither a
/// property table nor the gatherer's.
pub fn read_with_log(data: &[u8], log: &[u8]) -> Result<Index, Error> {
    if data.starts_with(SQLITE_SIGNATURE) {
        return windows_db::read(data, log);
    }
    let db = Database::open(data).map_err(|e| Error(e.to_string()))?;
    let mut problems = db.problems.clone();
    let table = db.tables.iter().find(|t| is_property_table(t));
    let has_gatherer = db.tables.iter().any(|t| t.name.ends_with("_Gthr"));
    if table.is_none() && !has_gatherer {
        return Err(Error(
            "no SystemIndex_PropertyStore, SystemIndex_0A or SystemIndex_Gthr: not a Windows Search index".to_owned(),
        ));
    }
    let layout = table.map(layout);
    let items = match table.zip(layout) {
        Some((table, layout)) => items(&db, table, layout, &mut problems),
        None => Vec::new(),
    };
    let indexed: HashSet<i64> = items.iter().filter_map(|item| item.work_id).collect();
    let gathered = gather::read(&db, &indexed, &mut problems);
    Ok(Index {
        table: table.map(|t| t.name.clone()),
        layout,
        items,
        gathered,
        problems,
    })
}

/// The property table: an id column, and a column for an item's path or
/// address.
fn is_property_table(table: &Table) -> bool {
    let has_id = ID_COLUMNS
        .iter()
        .any(|&id| table.column_index(id).is_some());
    let has_item = table
        .columns
        .iter()
        .any(|column| ITEM_PROPERTIES.contains(&property_name(&column.name)));
    has_id && has_item
}

/// Windows 10's layout numbers its columns.
fn layout(table: &Table) -> Layout {
    let numbered = table
        .columns
        .iter()
        .any(|column| property_name(&column.name) != column.name);
    if numbered {
        Layout::PropertyStore
    } else {
        Layout::Legacy
    }
}

fn items(
    db: &Database<'_>,
    table: &Table,
    layout: Layout,
    problems: &mut Vec<String>,
) -> Vec<Item> {
    let decoder = Decoder {
        layout,
        types: Types::read(db, problems),
    };
    let Ok(mut rows) = db.rows(&table.name) else {
        return Vec::new();
    };
    let mut items = Vec::new();
    for row in &mut rows {
        items.push(Item::read(table, &row.values, &decoder, problems));
    }
    problems.extend(
        rows.problems()
            .iter()
            .map(|p| format!("{}: {p}", table.name)),
    );
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_both_indexes() {
        assert_eq!(detect("Windows.edb"), Some(Format::Ese));
        assert_eq!(
            detect(r"C:\ProgramData\Microsoft\Search\Data\Applications\Windows\windows.EDB"),
            Some(Format::Ese)
        );
        assert_eq!(detect("evidence/Windows.edb"), Some(Format::Ese));
        assert_eq!(detect("Windows.db"), Some(Format::Sqlite));
        assert_eq!(detect("SRUDB.dat"), None);
        assert_eq!(detect("MyWindows.edb"), None);
    }
}
