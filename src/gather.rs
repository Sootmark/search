//! The gatherer's tables: `SystemIndex_Gthr`, one row per item the indexer
//! found (its file name, when it was last modified, the folder it is in as
//! a `ScopeID`, its `DocumentID`, which is the item's WorkID), and
//! `SystemIndex_GthrPth`, the folders: each `Scope` a `Name` (`Users/`)
//! under a `Parent` scope. Joined, they give each gathered item's full
//! address (`file:C:/Users/Public/Desktop/desktop.ini`,
//! `iehistory://{S-1-5-21-…}/http://www.bing.com/…`).
//!
//! The gatherer keeps rows the property table may lack: items found but not
//! yet indexed, or no longer.

use std::collections::{HashMap, HashSet};

use common::time::Ts;
use ese::{Database, Table, Value};

/// An item the indexer's gatherer found, from `SystemIndex_Gthr`.
#[derive(Debug, Clone, PartialEq)]
pub struct Gathered {
    /// `ScopeID`: the folder it is in, in `SystemIndex_GthrPth`.
    pub scope: Option<i64>,
    /// `DocumentID`: the WorkID of the item it was indexed as.
    pub document_id: Option<i64>,
    /// `FileName`: its name, or for web history the rest of its address.
    pub file_name: Option<String>,
    /// The folder's address, rebuilt from `SystemIndex_GthrPth`
    /// (`file:C:/Users/Public/`); `None` when the scope isn't there.
    pub folder: Option<String>,
    /// `LastModified`: when it was last modified, as the gatherer saw it
    /// (UTC).
    pub modified: Option<Ts>,
    /// Whether the property table holds an item with this WorkID; if not,
    /// it was found but not (or no longer) indexed.
    pub indexed: bool,
}

impl Gathered {
    /// The full address: the folder's, then the file name.
    #[must_use]
    pub fn path(&self) -> Option<String> {
        Some(format!(
            "{}{}",
            self.folder.as_ref()?,
            self.file_name.as_ref()?
        ))
    }
}

/// Read the gatherer's tables, if the database has them. `indexed` holds
/// the WorkID of each of the property table's items.
pub(crate) fn read(
    db: &Database<'_>,
    indexed: &HashSet<i64>,
    problems: &mut Vec<String>,
) -> Vec<Gathered> {
    let Some(table) = db.tables.iter().find(|t| t.name.ends_with("_Gthr")) else {
        return Vec::new();
    };
    let folders = Folders::read(db, problems);
    let Ok(mut rows) = db.rows(&table.name) else {
        return Vec::new();
    };
    let mut gathered = Vec::new();
    for row in &mut rows {
        let get = |name: &str| row.get(table, name);
        let scope = get("ScopeID").and_then(Value::as_i64);
        let document_id = get("DocumentID").and_then(Value::as_i64);
        gathered.push(Gathered {
            scope,
            document_id,
            file_name: get("FileName").and_then(Value::as_text).map(str::to_owned),
            folder: scope.and_then(|scope| folders.address(scope)),
            modified: get("LastModified").and_then(modified),
            indexed: document_id.is_some_and(|id| indexed.contains(&id)),
        });
    }
    problems.extend(
        rows.problems()
            .iter()
            .map(|p| format!("{}: {p}", table.name)),
    );
    gathered
}

/// `LastModified`: a FILETIME, big-endian in every version seen.
fn modified(value: &Value) -> Option<Ts> {
    let ticks = match value {
        Value::Binary(bytes) => u64::from_be_bytes(bytes.as_slice().try_into().ok()?),
        other => u64::try_from(other.as_i64()?).ok()?,
    };
    Some(Ts::from_filetime(ticks))
}

/// The addresses of `SystemIndex_GthrPth`'s folders, by scope.
struct Folders(HashMap<i64, String>);

impl Folders {
    fn read(db: &Database<'_>, problems: &mut Vec<String>) -> Self {
        let mut parents = HashMap::new();
        if let Some(table) = db.tables.iter().find(|t| t.name.ends_with("_GthrPth")) {
            read_parents(db, table, &mut parents, problems);
        }
        Self(addresses(&parents, problems))
    }

    fn address(&self, scope: i64) -> Option<String> {
        self.0.get(&scope).cloned()
    }
}

fn read_parents(
    db: &Database<'_>,
    table: &Table,
    parents: &mut HashMap<i64, (i64, String)>,
    problems: &mut Vec<String>,
) {
    let Ok(mut rows) = db.rows(&table.name) else {
        return;
    };
    for row in &mut rows {
        let get = |name: &str| row.get(table, name);
        let scope = get("Scope").and_then(Value::as_i64);
        let parent = get("Parent").and_then(Value::as_i64);
        let name = get("Name").and_then(Value::as_text);
        if let (Some(scope), Some(parent), Some(name)) = (scope, parent, name) {
            parents.insert(scope, (parent, name.to_owned()));
        }
    }
    problems.extend(
        rows.problems()
            .iter()
            .map(|p| format!("{}: {p}", table.name)),
    );
}

/// Every scope's address: its ancestors' names then its own, from the
/// first scope whose parent isn't listed. A scope in a cycle of parents, or
/// under one, has none.
fn addresses(
    parents: &HashMap<i64, (i64, String)>,
    problems: &mut Vec<String>,
) -> HashMap<i64, String> {
    let mut addresses: HashMap<i64, String> = HashMap::new();
    let mut unaddressable: HashSet<i64> = HashSet::new();
    for &scope in parents.keys() {
        let (chain, prefix) = match walk_up(scope, parents, &addresses, &unaddressable) {
            Ok(found) => found,
            Err(chain) => {
                unaddressable.extend(chain);
                continue;
            }
        };
        let mut address = prefix;
        for scope in chain.iter().rev() {
            address.push_str(&parents[scope].1);
            addresses.insert(*scope, address.clone());
        }
    }
    if !unaddressable.is_empty() {
        problems.push(format!(
            "SystemIndex_GthrPth: {} folders in or under a cycle of parents, left without an address",
            unaddressable.len()
        ));
    }
    addresses
}

/// The scopes from `scope` up to the root or to a scope whose address is
/// known, and that address (empty at the root); or, met a cycle or a scope
/// known to be in one, the scopes walked.
fn walk_up(
    scope: i64,
    parents: &HashMap<i64, (i64, String)>,
    addresses: &HashMap<i64, String>,
    unaddressable: &HashSet<i64>,
) -> Result<(Vec<i64>, String), Vec<i64>> {
    let mut chain = Vec::new();
    let mut walked = HashSet::new();
    let mut at = scope;
    while let Some((parent, _)) = parents.get(&at) {
        if let Some(known) = addresses.get(&at) {
            return Ok((chain, known.clone()));
        }
        if unaddressable.contains(&at) || !walked.insert(at) {
            return Err(chain);
        }
        chain.push(at);
        at = *parent;
    }
    Ok((chain, String::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(rows: &[(i64, i64, &str)]) -> HashMap<i64, (i64, String)> {
        rows.iter()
            .map(|&(scope, parent, name)| (scope, (parent, name.to_owned())))
            .collect()
    }

    #[test]
    fn addresses_join_names_from_the_root() {
        let parents = tree(&[
            (2, 1, "file:"),
            (3, 2, "C:"),
            (4, 3, "/"),
            (19, 4, "Users/"),
        ]);
        let mut problems = Vec::new();
        let addresses = addresses(&parents, &mut problems);
        assert_eq!(addresses[&19], "file:C:/Users/");
        assert_eq!(addresses[&3], "file:C:");
        assert!(problems.is_empty());
    }

    #[test]
    fn cycles_have_no_address() {
        let parents = tree(&[(2, 3, "a/"), (3, 2, "b/"), (4, 1, "c/")]);
        let mut problems = Vec::new();
        let addresses = addresses(&parents, &mut problems);
        assert_eq!(addresses.get(&2), None);
        assert_eq!(addresses[&4], "c/");
        assert_eq!(problems.len(), 1);
    }

    #[test]
    fn last_modified_is_big_endian() {
        let stored = Value::Binary(vec![0x01, 0xd0, 0xd1, 0xfb, 0x76, 0x93, 0xf9, 0x71]);
        let iso = modified(&stored).and_then(|ts| ts.to_iso8601());
        assert_eq!(iso.as_deref(), Some("2015-08-08T16:58:35.0150001Z"));
    }
}
