//! SIDR's test `Windows.edb` (Apache-2.0, a Windows 10 search index,
//! downloaded by `tests/fetch-sidr.sh`; skipped without it): the property
//! store layout, every item and summary read, a known item.

use search::Layout;

#[test]
fn windows_10_index() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/sidr/Windows.edb"
    );
    let Ok(data) = std::fs::read(path) else {
        return;
    };
    let index = search::read(&data).unwrap();
    assert_eq!(index.layout, Some(Layout::PropertyStore));
    assert_eq!((index.items.len(), index.gathered.len()), (1182, 1184));
    // A dirty database, nothing else wrong: ESE's unflagged compressed
    // summaries (sootmark-ese 0.1.1) read whole.
    assert_eq!(index.problems.len(), 1, "{:?}", index.problems);
    assert!(index.problems[0].starts_with("dirty shutdown"));
    assert_eq!(
        index.items.iter().filter(|i| i.summary.is_some()).count(),
        170
    );
    let desktop = index.items.iter().find(|i| i.work_id == Some(3)).unwrap();
    assert_eq!(
        desktop.path.as_deref(),
        Some(r"C:\Users\Public\Public Desktop\desktop.ini")
    );
    assert_eq!(desktop.size, Some(174));
    assert_eq!(
        desktop.modified.and_then(|t| t.to_iso8601()).as_deref(),
        Some("2019-12-07T09:12:42.7471994Z")
    );
}
