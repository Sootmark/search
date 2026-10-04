//! plaso's `Windows.edb` (Apache-2.0, `tests/fixtures/plaso/`), a Windows 7
//! index: items, values and gathered entries confirmed from libesedb's
//! reading of its raw values (the `sootmark-ese` oracle) and from
//! `esedbexport`'s decoding (`tests/oracle/`).

mod support;

use common::time::Ts;
use search::{Gathered, Index, Item, Layout, Property};

fn index() -> Index {
    search::read(&support::windows_edb()).unwrap()
}

fn iso(ts: Option<Ts>) -> String {
    ts.and_then(|ts| ts.to_iso8601()).unwrap_or_default()
}

fn item(index: &Index, work_id: i64) -> &Item {
    index
        .items
        .iter()
        .find(|item| item.work_id == Some(work_id))
        .unwrap()
}

fn gathered(index: &Index, document_id: i64) -> &Gathered {
    index
        .gathered
        .iter()
        .find(|g| g.document_id == Some(document_id))
        .unwrap()
}

#[test]
fn the_table_and_its_layout() {
    let index = index();
    assert_eq!(index.table.as_deref(), Some("SystemIndex_0A"));
    assert_eq!(index.layout, Some(Layout::Legacy));
    assert_eq!((index.items.len(), index.gathered.len()), (322, 339));
    // The database was copied dirty; nothing else is wrong with it.
    assert_eq!(index.problems.len(), 1);
    assert!(index.problems[0].starts_with("dirty shutdown"));
}

#[test]
fn a_shortcut() {
    let index = index();
    let shortcut = item(&index, 3);
    assert_eq!(
        shortcut.path.as_deref(),
        Some(r"C:\ProgramData\Microsoft\Windows\Start Menu\Default Programs.lnk")
    );
    assert_eq!(
        shortcut.folder.as_deref(),
        Some(r"C:\ProgramData\Microsoft\Windows\Start Menu")
    );
    assert_eq!(shortcut.file_name.as_deref(), Some("Default Programs.lnk"));
    assert_eq!(
        shortcut.url.as_deref(),
        Some("file:C:/ProgramData/Microsoft/Windows/Start Menu/Default Programs.lnk")
    );
    assert_eq!(shortcut.item_type.as_deref(), Some(".lnk"));
    assert_eq!(shortcut.item_type_text.as_deref(), Some("Shortcut"));
    assert_eq!(shortcut.kind, ["link", "program"]);
    // Big-endian, 00 00 00 00 00 00 05 02.
    assert_eq!(shortcut.size, Some(1282));
    assert_eq!(shortcut.attributes, Some(32));
    // 01 ca 04 40 1c c3 5f 15, big-endian.
    assert_eq!(iso(shortcut.modified), "2009-07-14T05:01:14.0464405Z");
    assert_eq!(iso(shortcut.created), "2009-07-14T05:01:14.0464405Z");
    assert_eq!(iso(shortcut.gather_time), "2015-08-24T11:23:40.5947052Z");
    assert_eq!(shortcut.owner.as_deref(), Some(r"NT AUTHORITY\SYSTEM"));
    assert_eq!(shortcut.computer.as_deref(), Some("STUDENT-PC1"));
    assert_eq!(
        shortcut.summary.as_deref(),
        Some(
            "Choose which programs you want Windows to use for activities like web browsing, \
             editing photos, sending e-mail, and playing music."
        )
    );
    assert_eq!(
        shortcut.property("System_Link_TargetParsingPath"),
        Some(&Property::Text(
            r"C:\Windows\system32\control.exe".to_owned()
        ))
    );
}

#[test]
fn a_folder_has_no_size() {
    let index = index();
    let users = item(&index, 2);
    assert_eq!(users.path.as_deref(), Some(r"C:\Users"));
    assert_eq!(users.kind, ["folder"]);
    // NULL in the record (libesedb reads the filler, 2a 2a …).
    assert_eq!(users.size, None);
    assert_eq!(iso(users.modified), "2015-08-08T16:58:35.0150000Z");
}

#[test]
fn summaries_in_runs_of_characters() {
    let index = index();
    // Compressed as runs sharing a high byte: U+2019 is a run of its own.
    assert_eq!(
        item(&index, 229).summary.as_deref(),
        Some("A chicken is a type of bird. It\u{2019}s tasty.")
    );
    let summaries = index.items.iter().filter(|i| i.summary.is_some()).count();
    assert_eq!(summaries, 81);
}

#[test]
fn a_page_of_internet_explorer_s_history() {
    let index = index();
    let page = item(&index, 251);
    assert_eq!(
        page.url.as_deref(),
        Some(
            "iehistory://{S-1-5-21-226059406-2984137831-1201299043-1107}/\
             http://webmail.student.greendale.xyz/src/login.php"
        )
    );
    assert_eq!(page.title.as_deref(), Some("SquirrelMail - Login"));
    assert_eq!(
        page.property("Microsoft_IE_TargetUrl"),
        Some(&Property::Text(
            "http://webmail.student.greendale.xyz/src/login.php".to_owned()
        ))
    );
    assert_eq!(
        page.property("Microsoft_IE_VisitCount"),
        Some(&Property::Integer(5))
    );
    let visited = page.property("System_ItemDate").and_then(Property::as_time);
    assert_eq!(iso(visited), "2015-09-05T18:42:34.6839332Z");
}

#[test]
fn gathered_items_and_their_paths() {
    let index = index();
    let shortcut = gathered(&index, 3);
    assert_eq!(shortcut.scope, Some(8));
    assert_eq!(
        shortcut.path().as_deref(),
        Some("file:C:/ProgramData/Microsoft/Windows/Start Menu/Default Programs.lnk")
    );
    assert_eq!(iso(shortcut.modified), "2015-08-24T11:23:25.0478310Z");
    assert!(shortcut.indexed);

    let page = gathered(&index, 217);
    assert_eq!(
        page.path().as_deref(),
        Some("iehistory://{S-1-5-21-226059406-2984137831-1201299043-1107}/http://www.msn.com/de-ch/?ocid=iehp")
    );
    assert_eq!(iso(page.modified), "2015-09-16T19:10:27.2094629Z");
    // The path the gatherer rebuilds is the item's address.
    assert_eq!(page.path(), item(&index, 217).url);
}

#[test]
fn gathered_but_not_indexed() {
    let index = index();
    let pending: Vec<&Gathered> = index.gathered.iter().filter(|g| !g.indexed).collect();
    // 17 pages of history, found but never indexed, none modified.
    assert_eq!(pending.len(), 17);
    assert!(pending.iter().all(|g| g.modified.is_none()));
    assert!(pending.iter().all(|g| g
        .path()
        .is_some_and(|path| path.starts_with("iehistory://"))));
    assert_eq!(
        gathered(&index, 323).path().as_deref(),
        Some(
            "iehistory://{S-1-5-21-226059406-2984137831-1201299043-1107}/\
             http://webmail.student.greendale.xyz/src/read_body.php?mailbox=INBOX&passed_id=24&startMessage=1"
        )
    );
}

#[test]
fn every_gathered_item_has_a_path_and_matches_its_item() {
    let index = index();
    assert!(index.gathered.iter().all(|g| g.path().is_some()));
    // For each of the 322 indexed items, the path rebuilt from the
    // gatherer's tables is the item's own address.
    let differing: Vec<_> = index
        .gathered
        .iter()
        .filter(|g| g.indexed)
        .filter_map(|g| {
            let item = item(&index, g.document_id?);
            (g.path() != item.url).then(|| (g.path(), item.url.clone()))
        })
        .collect();
    assert_eq!(differing, Vec::new());
}
