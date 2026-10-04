//! SIDR's test `Windows.edb` (Apache-2.0, a Windows 10 search index,
//! downloaded by `tests/fetch-sidr.sh`; skipped without it): the property
//! store layout, every item and summary read, a known item.

use search::{Layout, Property};

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

/// SIDR's `Windows.db` (a Windows 11 index): every value of SIDR's three
/// reports of it (files, activity history, internet history; one JSON
/// object per item, keyed by property) read the same.
#[test]
fn windows_11_index_matches_sidr() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sidr/");
    let Ok(data) = std::fs::read(format!("{dir}Windows.db")) else {
        return;
    };
    let index = search::read(&data).unwrap();
    assert_eq!(index.layout, Some(Layout::Sqlite));
    assert_eq!(index.problems, Vec::<String>::new());
    assert!(index.gathered.is_empty());
    let mut items = 0;
    let mut values = 0;
    for report in ["File", "Activity_History", "Internet_History"] {
        let path = format!("{dir}DESKTOP-O47KVAD_{report}_Report.json");
        let reports = std::fs::read_to_string(path).unwrap();
        for line in reports.lines().filter(|line| !line.trim().is_empty()) {
            let mut expected = json::object(line);
            let (_, work_id) = expected.remove(0);
            let work_id: i64 = work_id.parse().unwrap();
            let item = index.items.iter().find(|i| i.work_id == Some(work_id));
            let read = properties(item.unwrap());
            for (name, value) in expected {
                // SIDR writes local times as UTC.
                let value = match value.strip_suffix('Z') {
                    Some(local) if name.contains("_Local") => local.to_owned(),
                    _ => value,
                };
                let found = read.iter().find(|(n, _)| *n == name).map(|(_, v)| v);
                assert_eq!(found, Some(&value), "item {work_id}, {name}");
                values += 1;
            }
            items += 1;
        }
    }
    assert_eq!((items, values), (index.items.len(), 21_659));
}

/// An item's properties as SIDR writes them: text, numbers, times in ISO
/// 8601.
fn properties(item: &search::Item) -> Vec<(String, String)> {
    let texts = [
        ("System_ItemPathDisplay", &item.path),
        ("System_ItemFolderPathDisplay", &item.folder),
        ("System_FileName", &item.file_name),
        ("System_ItemUrl", &item.url),
        ("System_ItemType", &item.item_type),
        ("System_ItemTypeText", &item.item_type_text),
        ("System_FileOwner", &item.owner),
        ("System_ComputerName", &item.computer),
        ("System_Title", &item.title),
        ("System_Search_AutoSummary", &item.summary),
    ];
    let times = [
        ("System_DateModified", item.modified),
        ("System_DateCreated", item.created),
        ("System_DateAccessed", item.accessed),
        ("System_Search_GatherTime", item.gather_time),
    ];
    let numbers = [
        ("System_Size", item.size),
        ("System_FileAttributes", item.attributes.map(u64::from)),
    ];
    let texts = texts
        .into_iter()
        .filter_map(|(name, text)| Some((name.to_owned(), text.clone()?)));
    let times = times
        .into_iter()
        .filter_map(|(name, time)| Some((name.to_owned(), time?.to_iso8601()?)));
    let numbers = numbers
        .into_iter()
        .filter_map(|(name, number)| Some((name.to_owned(), number?.to_string())));
    let other = item
        .other
        .iter()
        .map(|(name, value)| (name.clone(), show(value)));
    texts.chain(times).chain(numbers).chain(other).collect()
}

fn show(value: &Property) -> String {
    match value {
        Property::Text(text) => text.clone(),
        Property::Integer(number) => number.to_string(),
        Property::Unsigned(number) => number.to_string(),
        Property::Float(number) => number.to_string(),
        Property::Bool(value) => value.to_string(),
        Property::Time(time) => time.to_iso8601().unwrap_or_default(),
        Property::Guid(bytes) => format!("{bytes:02x?}"),
        Property::List(items) => items.iter().map(show).collect::<Vec<_>>().join("; "),
        Property::Bytes(bytes) => format!("{} bytes", bytes.len()),
    }
}

/// Just enough JSON for SIDR's reports: one flat object per line, of
/// strings and numbers.
mod json {
    /// An object's members in order, numbers as written.
    pub fn object(line: &str) -> Vec<(String, String)> {
        let mut chars = line.trim().chars().peekable();
        assert_eq!(chars.next(), Some('{'));
        let mut members = Vec::new();
        loop {
            match chars.next() {
                Some('"') => {}
                Some('}') => return members,
                other => panic!("unexpected {other:?} in {line}"),
            }
            let name = string(&mut chars);
            assert_eq!(chars.next(), Some(':'));
            let value = if chars.peek() == Some(&'"') {
                chars.next();
                string(&mut chars)
            } else {
                let mut number = String::new();
                while let Some(&c) = chars.peek().filter(|c| !matches!(c, ',' | '}')) {
                    number.push(c);
                    chars.next();
                }
                number
            };
            members.push((name, value));
            if chars.peek() == Some(&',') {
                chars.next();
            }
        }
    }

    /// A string's text, after its opening quote.
    fn string(chars: &mut impl Iterator<Item = char>) -> String {
        let mut text = String::new();
        let mut units = Vec::new();
        while let Some(c) = chars.next() {
            if c != '\\' {
                text.push_str(&String::from_utf16_lossy(&std::mem::take(&mut units)));
            }
            match c {
                '"' => return text,
                '\\' => match chars.next() {
                    Some('u') => {
                        let hex: String = chars.by_ref().take(4).collect();
                        units.push(u16::from_str_radix(&hex, 16).unwrap());
                    }
                    Some(escaped) => {
                        text.push_str(&String::from_utf16_lossy(&std::mem::take(&mut units)));
                        text.push(match escaped {
                            'n' => '\n',
                            'r' => '\r',
                            't' => '\t',
                            'b' => '\u{8}',
                            'f' => '\u{c}',
                            other => other,
                        });
                    }
                    None => break,
                },
                c => text.push(c),
            }
        }
        panic!("unterminated string")
    }
}
