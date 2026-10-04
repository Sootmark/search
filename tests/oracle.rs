//! Every item of plaso's `Windows.edb` against libesedb's `esedbexport`,
//! which decodes Windows Search's binary values itself
//! (`tests/oracle/Windows.edb.tsv.gz`, written by `tests/oracle/gen.sh`):
//! paths, names, types, kinds, sizes, attributes, the four times, owner,
//! computer, titles and the decoded summaries.

mod support;

use common::time::Ts;
use search::Item;

/// The oracle's lines, each split at tabs: the column names first.
fn oracle() -> Vec<Vec<String>> {
    let compressed = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/oracle/Windows.edb.tsv.gz"
    ))
    .unwrap();
    let mut text = String::new();
    std::io::Read::read_to_string(
        &mut common::gzip::Decoder::new(compressed.as_slice()),
        &mut text,
    )
    .unwrap();
    text.lines()
        .map(|line| line.split('\t').map(unescape).collect())
        .collect()
}

/// A cell's text, its backslash escapes undone.
fn unescape(cell: &str) -> String {
    let mut text = String::with_capacity(cell.len());
    let mut chars = cell.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            text.push(c);
            continue;
        }
        match chars.next() {
            Some('t') => text.push('\t'),
            Some('n') => text.push('\n'),
            Some('r') => text.push('\r'),
            Some(other) => text.push(other),
            None => {}
        }
    }
    text
}

fn iso(ts: Option<Ts>) -> Option<String> {
    ts.and_then(|ts| ts.to_iso8601())
}

/// This crate's reading of `item`'s property `name`, as the oracle writes
/// it ("-" for none).
fn ours(item: &Item, name: &str) -> String {
    let text = |value: &Option<String>| value.clone();
    let value = match name {
        "DocID" => item.work_id.map(|id| id.to_string()),
        "System_ItemPathDisplay" => text(&item.path),
        "System_ItemFolderPathDisplay" => text(&item.folder),
        "System_FileName" => text(&item.file_name),
        "System_ItemUrl" => text(&item.url),
        "System_ItemType" => text(&item.item_type),
        "System_ItemTypeText" => text(&item.item_type_text),
        "System_Kind" => (!item.kind.is_empty()).then(|| item.kind.join("; ")),
        "System_Size" => item.size.map(|size| size.to_string()),
        "System_FileAttributes" => item.attributes.map(|a| a.to_string()),
        "System_DateModified" => iso(item.modified),
        "System_DateCreated" => iso(item.created),
        "System_DateAccessed" => iso(item.accessed),
        "System_Search_GatherTime" => iso(item.gather_time),
        "System_FileOwner" => text(&item.owner),
        "System_ComputerName" => text(&item.computer),
        "System_Title" => text(&item.title),
        "System_Search_AutoSummary" => text(&item.summary),
        other => panic!("no such column {other}"),
    };
    value.unwrap_or_else(|| "-".to_owned())
}

/// What the oracle says of `name` in `row`, mapped to what this crate reads
/// by design: an item's title is Internet Explorer's when it has no
/// `System_Title`; libesedb ignores the NULL bitmap of fixed columns and
/// reads NULL values from the record's filler bytes (0x2a): sizes
/// "********", attributes 0x2a2a2a2a.
fn expected(names: &[String], row: &[String], name: &str) -> String {
    let cell = |name: &str| {
        let at = names.iter().position(|n| n == name).unwrap();
        row[at].as_str()
    };
    if name == "System_Title" && cell(name) == "-" {
        return cell("Microsoft_IE_Title").to_owned();
    }
    let cell = cell(name);
    let filler = match name {
        "System_Size" => "********",
        "System_FileAttributes" => "707406378",
        _ => return cell.to_owned(),
    };
    if cell == filler {
        "-".to_owned()
    } else {
        cell.to_owned()
    }
}

#[test]
fn every_item_matches_esedbexport() {
    let index = search::read(&support::windows_edb()).unwrap();
    let lines = oracle();
    let (names, rows) = lines.split_first().unwrap();
    assert_eq!(index.items.len(), rows.len());
    let mut compared = 0;
    let mut differences = Vec::new();
    for (item, row) in index.items.iter().zip(rows) {
        // Internet Explorer's titles are compared as the items' titles.
        for name in names.iter().filter(|&name| name != "Microsoft_IE_Title") {
            let (ours, theirs) = (ours(item, name), expected(names, row, name));
            compared += usize::from(theirs != "-");
            if ours != theirs {
                differences.push(format!("{:?} {name}: {ours:?} != {theirs:?}", item.work_id));
            }
        }
    }
    assert_eq!(differences, Vec::<String>::new());
    // 322 items, 18 columns: the values esedbexport prints, its filler
    // aside, and 61 titles from Internet Explorer.
    assert_eq!(compared, 4_598);
}
