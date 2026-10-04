# search

The Windows Search index (`ProgramData\Microsoft\Search\Data\Applications\Windows\Windows.edb`), for forensics: what the indexer knew of every file, folder, e-mail and page of browser history it indexed. Paths, sizes, times, owners and the first words of documents, sometimes kept after the files themselves were deleted; and the gatherer's list of every item it found, with its full path rebuilt. Read with [`sootmark-ese`](https://crates.io/crates/sootmark-ese), a reader of ESE databases written from the format; no other dependency but its sibling `sootmark-common` (times).

```toml
[dependencies]
sootmark-search = "0.1"
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

- `read(bytes)`: the property table, one `Item` per indexed item, whichever the layout: `SystemIndex_PropertyStore` (Windows 10, columns named with the property's number, `4447-System_ItemPathDisplay`, `13F-System_Size`) or `SystemIndex_0A` (Windows Vista and 7, `System_ItemPathDisplay`; Windows 8 not seen), told apart by their columns:
  - `work_id` (`WorkID`, `DocID` before Windows 10);
  - `path`, `folder`, `file_name` (`System.ItemPathDisplay`, `System.ItemFolderPathDisplay`, `System.FileName`), `url` (`System.ItemUrl`: `file:C:/…`, `iehistory://{SID}/http://…`, `csc://…`);
  - `item_type`, `item_type_text`, `kind` (`.lnk`, `Shortcut`, `link` and `program`);
  - `size`, `attributes`; `modified`, `created`, `accessed` and `gather_time` (when the indexer last gathered it), UTC;
  - `owner` (`NT AUTHORITY\SYSTEM`), `computer`, `title` (`System.Title`, else Internet Explorer's title for a page of its history), and `summary` (`System.Search.AutoSummary`: the start of the item's text, as indexed);
  - `other`: every other property, by name (`System_Link_TargetParsingPath`, `Microsoft_IE_VisitCount`, `System_ItemDate`), as text, numbers, times, GUIDs or lists; a binary value this crate can't read is kept as bytes.
- Windows Search's own storage: 8-byte FILETIMEs and sizes in binary columns, big-endian in `SystemIndex_0A` and little-endian in `SystemIndex_PropertyStore`, typed from `SystemIndex_0P` where the database has it and from a list of well-known properties where not; lists of strings stored as UTF-16 with NULs between; and the summary that Windows Vista and 7 compress and obfuscate: the obfuscation as documented by Joachim Metz ("Windows Search forensics", 2010), the compression (one byte per character, or runs of characters sharing their high byte) worked out on the test database and checked against libesedb.
- `gathered`: every row of `SystemIndex_Gthr` as a `Gathered`: `ScopeID`, `DocumentID` (the item's WorkID), `FileName`, `LastModified`, the folder's address rebuilt from `SystemIndex_GthrPth` (`file:C:/Users/Public/Desktop/`) and `path()`, the full address; `indexed` says whether the property table holds that WorkID, so items found but never (or no longer) indexed stand out.
- `detect(name)`: whether a file is a Windows Search index (`Windows.edb`).
- Damage goes to `problems`, never a panic; only a file that isn't an ESE database, or has neither a property table nor the gatherer's, is an error.

## Not yet

- Windows 11's index (`Windows.db`, `Windows-gather.db`): SQLite, not ESE.
- Summaries in compression methods other than the two the test database holds (other first bytes, which `MSSUncompressText` may produce but no openly licensed database holds): reported, kept as bytes.
- Typing properties of `SystemIndex_PropertyStore` beyond the well-known list: Windows 10 doesn't list its properties' types in a readable table, so an unknown binary property stays bytes there.
- The full-text tables (`SystemIndex_1_DATA_*`, `SystemIndex_1_OCC_*`), Windows 10's `ChangeTracking`, the gatherer's `UserData`, and the transaction logs (`MSS*.log`), whose changes a dirty database may lack (`sootmark-ese` reports a dirty shutdown).

## How it's checked

- SIDR's test `Windows.edb` (Apache-2.0, a Windows 10 index; `tests/fetch-sidr.sh` downloads it at a pinned commit and checksum, CI caches it): the property-store layout, its 1,182 items and 1,184 gathered entries, all 170 summaries read whole, a known item's path, size and time. Its items were also compared once with SIDR's own report (980 items, 6,711 values, equal).
- plaso's `Windows.edb` (Apache-2.0, `tests/fixtures/plaso/`, see `NOTICE`), a Windows 7 index: every one of its 322 items against libesedb's `esedbexport`, which decodes Windows Search's binary values itself (`tests/oracle/`, written by `gen.sh`): paths, names, URLs, types, kinds, sizes, attributes, the four times to the 100 ns, owners, computers, titles and the 81 decoded summaries, 4,598 values, all equal. Its NULL sizes and attributes, which libesedb reads from the record's filler bytes, are NULL here by design (as in `sootmark-ese`).
- On the same file, values confirmed from libesedb's reading of the raw columns (`sootmark-ese`'s oracle): a shortcut, a folder, a page of Internet Explorer's history with its visit count and date, summaries in both compressions; the 339 gathered items, every one with a rebuilt path equal to its item's address, and the 17 pages of history the gatherer found but never indexed.
- A Windows 10 index outside the repository (the test data of [SIDR](https://github.com/strozfriedberg/sidr), Apache-2.0): 980 items against SIDR's own report, 6,711 values (paths, types, the four times, sizes, owners, summaries) equal but for the 115 long summaries above.
- Property tests: arbitrary bytes, and the database damaged anywhere in its used pages or cut anywhere, give items, problems or an error, never a panic.

## Licence

MIT or Apache-2.0, at your option. The test database under `tests/fixtures/plaso/` keeps plaso's licence (Apache-2.0); libesedb (LGPL) is only run to write the expected values, never included.
