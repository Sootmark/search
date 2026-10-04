# search

The Windows Search index (`ProgramData\Microsoft\Search\Data\Applications\Windows\Windows.edb`, `Windows.db` from Windows 11), for forensics: what the indexer knew of every file, folder, e-mail and page of browser history it indexed. Paths, sizes, times, owners and the first words of documents, sometimes kept after the files themselves were deleted; and the gatherer's list of every item it found, with its full path rebuilt. Read with [`sootmark-ese`](https://crates.io/crates/sootmark-ese) and [`sootmark-sqlite`](https://crates.io/crates/sootmark-sqlite), readers of ESE and SQLite databases written from their formats; no other dependency but their sibling `sootmark-common` (times).

```toml
[dependencies]
sootmark-search = "0.2"
```

```rust
let index = search::read(&std::fs::read("Windows.edb")?)?;
for item in &index.items {
    println!("{:?} {:?} {:?} {:?}", item.path, item.size, item.modified, item.summary);
}
for found in index.gathered.iter().filter(|g| !g.indexed) {
    println!("found, not indexed: {:?}", found.path());
}
```

## What you get

- `read(bytes)`: the property table, one `Item` per indexed item, whichever the layout: `SystemIndex_PropertyStore` (Windows 10, columns named with the property's number, `4447-System_ItemPathDisplay`, `13F-System_Size`) or `SystemIndex_0A` (Windows Vista and 7, `System_ItemPathDisplay`; Windows 8 not seen), told apart by their columns, or Windows 11's `Windows.db` (SQLite: `SystemIndex_1_PropertyStore`, a row per property of an item, named in `SystemIndex_1_PropertyStore_Metadata`), told apart by its signature. `read_with_log(bytes, wal)` reads `Windows.db` with its write-ahead log.
  - `work_id` (`WorkID`, `DocID` before Windows 10);
  - `path`, `folder`, `file_name` (`System.ItemPathDisplay`, `System.ItemFolderPathDisplay`, `System.FileName`), `url` (`System.ItemUrl`: `file:C:/…`, `iehistory://{SID}/http://…`, `csc://…`);
  - `item_type`, `item_type_text`, `kind` (`.lnk`, `Shortcut`, `link` and `program`);
  - `size`, `attributes`; `modified`, `created`, `accessed` and `gather_time` (when the indexer last gathered it), UTC; the activity history's local times (`System_ActivityHistory_LocalStartTime`) without a zone, as they are;
  - `owner` (`NT AUTHORITY\SYSTEM`), `computer`, `title` (`System.Title`, else Internet Explorer's title for a page of its history), and `summary` (`System.Search.AutoSummary`: the start of the item's text, as indexed);
  - `other`: every other property, by name (`System_Link_TargetParsingPath`, `Microsoft_IE_VisitCount`, `System_ItemDate`), as text, numbers, times, GUIDs or lists; a binary value this crate can't read is kept as bytes.
- Windows Search's own storage: 8-byte FILETIMEs and sizes in binary columns, big-endian in `SystemIndex_0A` and little-endian in `SystemIndex_PropertyStore`, typed from `SystemIndex_0P` or `Windows.db`'s metadata where the database has them and from a list of well-known properties where not (or where Windows 11 types a time as a number); lists of strings stored as UTF-16 with NULs between; and the summary that Windows Vista and 7 compress and obfuscate: the obfuscation as documented by Joachim Metz ("Windows Search forensics", 2010), the compression (one byte per character, or runs of characters sharing their high byte) worked out on the test database and checked against libesedb.
- `gathered`: every row of `SystemIndex_Gthr` as a `Gathered`: `ScopeID`, `DocumentID` (the item's WorkID), `FileName`, `LastModified`, the folder's address rebuilt from `SystemIndex_GthrPth` (`file:C:/Users/Public/Desktop/`) and `path()`, the full address; `indexed` says whether the property table holds that WorkID, so items found but never (or no longer) indexed stand out.
- `detect(name)`: which index a file is, by name (`Windows.edb`: `Format::Ese`; `Windows.db`: `Format::Sqlite`).
- Damage goes to `problems`, never a panic; only a file that is neither an ESE nor a SQLite database, or has neither a property table nor the gatherer's, is an error.

## Not yet

- Windows 11's gatherer database (`Windows-gather.db`), and deleted rows of `Windows.db` (`sootmark-sqlite` can recover them).
- Summaries in compression methods other than the two the test database holds (other first bytes, which `MSSUncompressText` may produce but no openly licensed database holds): reported, kept as bytes.
- Typing properties of `SystemIndex_PropertyStore` beyond the well-known list: Windows 10 doesn't list its properties' types in a readable table, so an unknown binary property stays bytes there.
- The full-text tables (`SystemIndex_1_DATA_*`, `SystemIndex_1_OCC_*`), Windows 10's `ChangeTracking`, the gatherer's `UserData`, and the transaction logs (`MSS*.log`), whose changes a dirty database may lack (`sootmark-ese` reports a dirty shutdown).

## How it's checked

- SIDR's test `Windows.edb` (Apache-2.0, a Windows 10 index; `tests/fetch-sidr.sh` downloads it at a pinned commit and checksum, CI caches it): the property-store layout, its 1,182 items and 1,184 gathered entries, all 170 summaries read whole, a known item's path, size and time. Its items were also compared once with SIDR's own report (980 items, 6,711 values, equal).
- SIDR's test `Windows.db` (a Windows 11 index, downloaded likewise with SIDR's three reports of it): every value of every report, 21,659 values of all 839 items (paths, types, kinds, sizes, attributes, every time, GUIDs, activity history, summaries), equal; SIDR writes the activity history's local times with a `Z`, which the test drops.
- plaso's `Windows.edb` (Apache-2.0, `tests/fixtures/plaso/`, see `NOTICE`), a Windows 7 index: every one of its 322 items against libesedb's `esedbexport`, which decodes Windows Search's binary values itself (`tests/oracle/`, written by `gen.sh`): paths, names, URLs, types, kinds, sizes, attributes, the four times to the 100 ns, owners, computers, titles and the 81 decoded summaries, 4,598 values, all equal. Its NULL sizes and attributes, which libesedb reads from the record's filler bytes, are NULL here by design (as in `sootmark-ese`).
- On the same file, values confirmed from libesedb's reading of the raw columns (`sootmark-ese`'s oracle): a shortcut, a folder, a page of Internet Explorer's history with its visit count and date, summaries in both compressions; the 339 gathered items, every one with a rebuilt path equal to its item's address, and the 17 pages of history the gatherer found but never indexed.
- Property tests: arbitrary bytes (alone or after a SQLite header), plaso's database damaged anywhere in its used pages or cut anywhere, and SIDR's `Windows.db` damaged anywhere, give items, problems or an error, never a panic.

## Licence

MIT or Apache-2.0, at your option. The test database under `tests/fixtures/plaso/` keeps plaso's licence (Apache-2.0); libesedb (LGPL) is only run to write the expected values, never included.
