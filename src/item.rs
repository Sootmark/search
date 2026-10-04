//! An indexed item: one row of the property table, its well-known
//! properties read into fields, the rest kept by name.

use common::time::Ts;
use ese::{Table, Value};

use crate::property::{property_name, Decoder, Property};

/// The column with each item's id: `WorkID` from Windows 10, `DocID`
/// before.
pub(crate) const ID_COLUMNS: [&str; 2] = ["WorkID", "DocID"];

/// An item the index holds: a file, a folder, an e-mail, a page from the
/// browser's history, an app's activity.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Item {
    /// Its WorkID (`DocID` before Windows 10): the gatherer's
    /// `DocumentID`.
    pub work_id: Option<i64>,
    /// `System.ItemPathDisplay`: its path as Explorer shows it
    /// (`C:\Users\Public\Desktop\desktop.ini`).
    pub path: Option<String>,
    /// `System.ItemFolderPathDisplay`: the folder it is in.
    pub folder: Option<String>,
    /// `System.FileName`.
    pub file_name: Option<String>,
    /// `System.ItemUrl`: its address for the indexer (`file:C:/…`,
    /// `iehistory://…`, `mapi://…`, `winrt://…`).
    pub url: Option<String>,
    /// `System.ItemType`: an extension (`.lnk`) or a kind of item
    /// (`Directory`).
    pub item_type: Option<String>,
    /// `System.ItemTypeText`: the type as Explorer names it (`Shortcut`).
    pub item_type_text: Option<String>,
    /// `System.Kind`: what it is (`folder`, `link`, `program`, `email`).
    pub kind: Vec<String>,
    /// `System.Size`, in bytes.
    pub size: Option<u64>,
    /// `System.FileAttributes`.
    pub attributes: Option<u32>,
    /// `System.DateModified` (UTC).
    pub modified: Option<Ts>,
    /// `System.DateCreated` (UTC).
    pub created: Option<Ts>,
    /// `System.DateAccessed` (UTC).
    pub accessed: Option<Ts>,
    /// `System.Search.GatherTime`: when the indexer last gathered it (UTC).
    pub gather_time: Option<Ts>,
    /// `System.FileOwner`: the owning account (`NT AUTHORITY\SYSTEM`).
    pub owner: Option<String>,
    /// `System.ComputerName`.
    pub computer: Option<String>,
    /// `System.Title`: a document's, an e-mail's or a media file's title;
    /// for Internet Explorer's history, the page's (`Microsoft.IE.Title`).
    pub title: Option<String>,
    /// `System.Search.AutoSummary`: the start of its text, as indexed;
    /// decoded from Windows Search's compression where it is compressed.
    pub summary: Option<String>,
    /// Its other properties, by name as stored (`System_Link_TargetUrl`,
    /// `Microsoft_IE_VisitCount`), in table order; values this crate
    /// couldn't decode are kept as bytes.
    pub other: Vec<(String, Property)>,
}

impl Item {
    /// The value of property `name` among [`Item::other`]
    /// (`System_Link_TargetUrl`).
    #[must_use]
    pub fn property(&self, name: &str) -> Option<&Property> {
        self.other
            .iter()
            .find(|(property, _)| property == name)
            .map(|(_, value)| value)
    }

    /// Read an item from a row of the property table; values that don't
    /// decode go to `problems`.
    pub(crate) fn read(
        table: &Table,
        values: &[Value],
        decoder: &Decoder,
        problems: &mut Vec<String>,
    ) -> Self {
        let work_id = ID_COLUMNS
            .iter()
            .find_map(|&name| values.get(table.column_index(name)?)?.as_i64());
        let mut properties = Vec::new();
        for (column, value) in table.columns.iter().zip(values) {
            if ID_COLUMNS.contains(&column.name.as_str()) {
                continue;
            }
            let name = property_name(&column.name);
            let Some((property, problem)) = decoder.decode(name, value) else {
                continue;
            };
            if let Some(problem) = problem {
                let id = work_id.map_or_else(|| "?".to_owned(), |id| id.to_string());
                problems.push(format!("{}, item {id}, {name}: {problem}", table.name));
            }
            properties.push((name.to_owned(), property));
        }
        Self::from_properties(work_id, properties)
    }

    /// The well-known properties taken into fields, the rest left in
    /// [`Item::other`].
    pub(crate) fn from_properties(
        work_id: Option<i64>,
        mut other: Vec<(String, Property)>,
    ) -> Self {
        let mut text = |name| take(&mut other, name, |p| p.as_text().map(str::to_owned));
        let path = text("System_ItemPathDisplay");
        let folder = text("System_ItemFolderPathDisplay");
        let file_name = text("System_FileName");
        let url = text("System_ItemUrl");
        let item_type = text("System_ItemType");
        let item_type_text = text("System_ItemTypeText");
        let owner = text("System_FileOwner");
        let computer = text("System_ComputerName");
        let title = text("System_Title").or_else(|| text("Microsoft_IE_Title"));
        let summary = text("System_Search_AutoSummary");
        let mut time = |name| take(&mut other, name, Property::as_time);
        let modified = time("System_DateModified");
        let created = time("System_DateCreated");
        let accessed = time("System_DateAccessed");
        let gather_time = time("System_Search_GatherTime");
        Self {
            work_id,
            path,
            folder,
            file_name,
            url,
            item_type,
            item_type_text,
            kind: take(&mut other, "System_Kind", Property::texts).unwrap_or_default(),
            size: take(&mut other, "System_Size", Property::as_u64),
            attributes: take(&mut other, "System_FileAttributes", |p| {
                u32::try_from(p.as_u64()?).ok()
            }),
            modified,
            created,
            accessed,
            gather_time,
            owner,
            computer,
            title,
            summary,
            other,
        }
    }
}

/// Property `name`'s value as `read` makes it, taken out of `properties`;
/// left there when it doesn't read so.
fn take<T>(
    properties: &mut Vec<(String, Property)>,
    name: &str,
    read: impl Fn(&Property) -> Option<T>,
) -> Option<T> {
    let at = properties
        .iter()
        .position(|(property, _)| property == name)?;
    let value = read(&properties[at].1)?;
    properties.remove(at);
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn well_known_properties_become_fields() {
        let other = vec![
            (
                "System_FileName".to_owned(),
                Property::Text("a.txt".to_owned()),
            ),
            ("System_Size".to_owned(), Property::Unsigned(3)),
            (
                "System_Kind".to_owned(),
                Property::Text("document".to_owned()),
            ),
            ("System_Title".to_owned(), Property::Integer(1)),
            ("Microsoft_IE_VisitCount".to_owned(), Property::Integer(2)),
        ];
        let item = Item::from_properties(Some(7), other);
        assert_eq!(item.file_name.as_deref(), Some("a.txt"));
        assert_eq!(item.size, Some(3));
        assert_eq!(item.kind, ["document"]);
        // A title that isn't text stays among the others.
        assert_eq!(item.title, None);
        assert_eq!(item.property("System_Title"), Some(&Property::Integer(1)));
        assert_eq!(item.other.len(), 2);
    }
}
