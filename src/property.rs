//! Property values: what each column of the property table holds, read
//! from the way Windows Search stores it.
//!
//! The columns are named after Windows properties (`System.ItemPathDisplay`
//! written `System_ItemPathDisplay`), behind a property number from
//! Windows 10 on (`4447-System_ItemPathDisplay`, `13F-System_Size`). Most
//! values are ordinary ESE values. The rest are binary: 8-byte FILETIMEs and
//! 64-bit integers (big-endian in `SystemIndex_0A`, little-endian in
//! `SystemIndex_PropertyStore`), lists of UTF-16 strings, and text Windows
//! Search compressed and obfuscated itself ([`crate::encoded`]).

use std::collections::HashMap;

use common::time::Ts;
use ese::{Database, Value};

use crate::Layout;

// `VARTYPE`s of the properties whose binary values this crate reads.
const VT_I8: u32 = 20;
const VT_UI8: u32 = 21;
const VT_LPWSTR: u32 = 31;
const VT_FILETIME: u32 = 64;
const VT_CLSID: u32 = 72;
/// `VT_VECTOR`: a list of values of the type in the low bits.
const VT_VECTOR: u32 = 0x1000;

/// Properties stored as binary FILETIMEs, for databases that don't list
/// their properties' types (`SystemIndex_PropertyStore`): those Windows 7's
/// `SystemIndex_0P` gives the type `VT_FILETIME` and Windows 10 stores in
/// binary columns, and the times of the activity history and of links.
const FILETIMES: [&str; 31] = [
    "System_ActivityHistory_EndTime",
    "System_ActivityHistory_StartTime",
    "System_Calendar_ReminderTime",
    "System_Communication_DateItemExpires",
    "System_Contact_Anniversary",
    "System_Contact_Birthday",
    "System_DateAccessed",
    "System_DateAcquired",
    "System_DateArchived",
    "System_DateCompleted",
    "System_DateCreated",
    "System_DateImported",
    "System_DateModified",
    "System_Document_DateCreated",
    "System_Document_DatePrinted",
    "System_Document_DateSaved",
    "System_DueDate",
    "System_EndDate",
    "System_GPS_Date",
    "System_ItemDate",
    "System_Link_DateVisited",
    "System_Media_DateEncoded",
    "System_Message_DateReceived",
    "System_Message_DateSent",
    "System_Photo_DateTaken",
    "System_RecordedTV_DateContentExpires",
    "System_RecordedTV_OriginalBroadcastDate",
    "System_RecordedTV_RecordingTime",
    "System_Search_GatherTime",
    "System_Software_DateLastUsed",
    "System_StartDate",
];

/// Properties stored as binary 64-bit unsigned integers, likewise.
const UNSIGNED: [&str; 5] = [
    "System_Document_TotalEditingTime",
    "System_FileFRN",
    "System_Media_Duration",
    "System_Size",
    "System_ThumbnailCacheId",
];

/// Properties stored as binary text, likewise.
const TEXTS: [(&str, u32); 2] = [
    ("System_Kind", VT_VECTOR | VT_LPWSTR),
    ("System_Search_AutoSummary", VT_LPWSTR),
];

/// A property's value.
#[derive(Debug, Clone, PartialEq)]
pub enum Property {
    /// Text.
    Text(String),
    /// A signed integer (of any size), or a currency amount in 1/10,000.
    Integer(i64),
    /// An unsigned 64-bit integer (`VT_UI8`: sizes, ids).
    Unsigned(u64),
    /// A float.
    Float(f64),
    /// A boolean.
    Bool(bool),
    /// A time (`VT_FILETIME`, or an ESE date), UTC.
    Time(Ts),
    /// A GUID, its 16 bytes as stored.
    Guid([u8; 16]),
    /// Several values (`System_Kind`: `link`, `program`).
    List(Vec<Property>),
    /// Bytes this crate doesn't interpret, or couldn't (see the problems).
    Bytes(Vec<u8>),
}

impl Property {
    /// The text, if this is text.
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            _ => None,
        }
    }

    /// The time, if this is one.
    #[must_use]
    pub fn as_time(&self) -> Option<Ts> {
        match self {
            Self::Time(time) => Some(*time),
            _ => None,
        }
    }

    /// Any integer as a `u64`, if it is one and isn't negative.
    #[must_use]
    pub fn as_u64(&self) -> Option<u64> {
        match *self {
            Self::Unsigned(value) => Some(value),
            Self::Integer(value) => u64::try_from(value).ok(),
            _ => None,
        }
    }

    /// The texts of a list (or of a single text).
    #[must_use]
    pub fn texts(&self) -> Option<Vec<String>> {
        match self {
            Self::Text(text) => Some(vec![text.clone()]),
            Self::List(items) => items
                .iter()
                .map(|item| item.as_text().map(str::to_owned))
                .collect(),
            _ => None,
        }
    }
}

/// The property a column holds: its name without the property number
/// Windows 10 puts first (`4447-System_ItemPathDisplay`,
/// `13F-System_Size`).
pub(crate) fn property_name(column: &str) -> &str {
    match column.split_once('-') {
        Some((number, name)) if is_property_number(number) => name,
        _ => column,
    }
}

/// Digits, perhaps followed by letters (`13F`).
fn is_property_number(text: &str) -> bool {
    text.starts_with(|c: char| c.is_ascii_digit())
        && text.chars().all(|c| c.is_ascii_alphanumeric())
}

/// The types (`VARTYPE`s) of properties, by name.
pub(crate) struct Types(HashMap<String, u32>);

impl Types {
    /// The types `SystemIndex_0P` lists (Windows Vista and 7: `Name`
    /// `System.Size`, `Type` 21), and those this crate knows.
    pub(crate) fn read(db: &Database<'_>, problems: &mut Vec<String>) -> Self {
        let mut types = known_types();
        let Some(table) = db.tables.iter().find(|t| is_type_table(t)) else {
            return Self(types);
        };
        let Ok(mut rows) = db.rows(&table.name) else {
            return Self(types);
        };
        for row in &mut rows {
            let name = row.get(table, "Name").and_then(Value::as_text);
            let vt = row.get(table, "Type").and_then(Value::as_i64);
            if let (Some(name), Some(vt)) = (name, vt.and_then(|vt| u32::try_from(vt).ok())) {
                types.insert(name.replace('.', "_"), vt);
            }
        }
        problems.extend(
            rows.problems()
                .iter()
                .map(|p| format!("{}: {p}", table.name)),
        );
        Self(types)
    }

    fn of(&self, property: &str) -> Option<u32> {
        self.0.get(property).copied()
    }
}

/// The types this crate knows without a list.
fn known_types() -> HashMap<String, u32> {
    let times = FILETIMES.iter().map(|&name| (name, VT_FILETIME));
    let unsigned = UNSIGNED.iter().map(|&name| (name, VT_UI8));
    times
        .chain(unsigned)
        .chain(TEXTS)
        .map(|(name, vt)| (name.to_owned(), vt))
        .collect()
}

/// `SystemIndex_0P`: a property list, with `Name` and `Type` columns.
fn is_type_table(table: &ese::Table) -> bool {
    table.name.starts_with("SystemIndex_")
        && table.name.ends_with('P')
        && table.column_index("Name").is_some()
        && table.column_index("Type").is_some()
}

/// Reads stored values as properties.
pub(crate) struct Decoder {
    pub(crate) layout: Layout,
    pub(crate) types: Types,
}

impl Decoder {
    /// `value` of `property`; `None` when NULL. A binary value that should
    /// decode but doesn't is kept as bytes, with the reason.
    pub(crate) fn decode(
        &self,
        property: &str,
        value: &Value,
    ) -> Option<(Property, Option<String>)> {
        let decoded = match value {
            Value::Null => return None,
            Value::Binary(bytes) => return Some(self.binary(property, bytes)),
            Value::MultiValue(values) => return Some(self.list(property, values)),
            Value::Text(text) => Property::Text(text.clone()),
            Value::Bool(value) => Property::Bool(*value),
            Value::DateTime(days) => Property::Time(Ts::from_ole_date(*days)),
            Value::F32(_) | Value::F64(_) => Property::Float(value.as_f64().unwrap_or_default()),
            Value::Guid(bytes) => Property::Guid(*bytes),
            integer => Property::Integer(integer.as_i64().unwrap_or_default()),
        };
        Some((decoded, None))
    }

    /// The values of a multi-valued column, and what didn't decode.
    fn list(&self, property: &str, values: &[Value]) -> (Property, Option<String>) {
        let mut problems = Vec::new();
        let mut items = Vec::new();
        for (item, problem) in values.iter().filter_map(|v| self.decode(property, v)) {
            items.push(item);
            problems.extend(problem);
        }
        let problem = (!problems.is_empty()).then(|| problems.join("; "));
        (Property::List(items), problem)
    }

    /// A binary value, read by its property's type.
    fn binary(&self, property: &str, bytes: &[u8]) -> (Property, Option<String>) {
        let Some(vt) = self.types.of(property) else {
            return (Property::Bytes(bytes.to_vec()), None);
        };
        let decoded = match vt {
            VT_FILETIME => self
                .u64(bytes)
                .map(|ticks| Property::Time(Ts::from_filetime(ticks))),
            VT_UI8 => self.u64(bytes).map(Property::Unsigned),
            VT_I8 => self.u64(bytes).map(|value| Property::Integer(value as i64)),
            VT_CLSID => bytes.try_into().ok().map(Property::Guid),
            VT_LPWSTR => return self.text(bytes),
            vt if vt == VT_VECTOR | VT_LPWSTR => return self.texts(bytes),
            _ => return (Property::Bytes(bytes.to_vec()), None),
        };
        match decoded {
            Some(property) => (property, None),
            None => (
                Property::Bytes(bytes.to_vec()),
                Some(format!("{} bytes for a value of type {vt}", bytes.len())),
            ),
        }
    }

    /// A 64-bit number in the database's byte order.
    fn u64(&self, bytes: &[u8]) -> Option<u64> {
        let bytes: [u8; 8] = bytes.try_into().ok()?;
        Some(match self.layout {
            Layout::Legacy => u64::from_be_bytes(bytes),
            Layout::PropertyStore => u64::from_le_bytes(bytes),
        })
    }

    /// Binary text: compressed and obfuscated, or plain UTF-16.
    fn text(&self, bytes: &[u8]) -> (Property, Option<String>) {
        let decoded = match self.layout {
            Layout::Legacy => crate::encoded::decode(bytes),
            Layout::PropertyStore => Ok(utf16(bytes)),
        };
        match decoded {
            Ok(text) => (Property::Text(text), None),
            Err(problem) => (Property::Bytes(bytes.to_vec()), Some(problem)),
        }
    }

    /// A binary list of strings: in `SystemIndex_PropertyStore` UTF-16,
    /// the strings separated by NULs; in `SystemIndex_0A` (where no such
    /// value has been seen, lists being multi-valued text columns) taken as
    /// one compressed text.
    fn texts(&self, bytes: &[u8]) -> (Property, Option<String>) {
        if self.layout == Layout::Legacy {
            return self.text(bytes);
        }
        let joined = utf16(bytes);
        let items = joined
            .split('\0')
            .map(|text| Property::Text(text.to_owned()))
            .collect();
        (Property::List(items), None)
    }
}

/// UTF-16LE text, without the NULs that may end it.
fn utf16(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    text(&units)
}

/// UTF-16 units as text, without the NULs that may end it.
pub(crate) fn text(units: &[u16]) -> String {
    let end = units
        .iter()
        .rposition(|&unit| unit != 0)
        .map_or(0, |at| at + 1);
    String::from_utf16_lossy(&units[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decoder(layout: Layout) -> Decoder {
        Decoder {
            layout,
            types: Types(known_types()),
        }
    }

    #[test]
    fn property_numbers_are_dropped() {
        assert_eq!(
            property_name("4447-System_ItemPathDisplay"),
            "System_ItemPathDisplay"
        );
        assert_eq!(property_name("13F-System_Size"), "System_Size");
        assert_eq!(property_name("System_Size"), "System_Size");
        assert_eq!(property_name("WorkID"), "WorkID");
        assert_eq!(property_name("A-B"), "A-B");
    }

    #[test]
    fn numbers_follow_the_layout() {
        let stored = Value::Binary(vec![0, 0, 0, 0, 0, 0, 0x05, 0x02]);
        let legacy = decoder(Layout::Legacy).decode("System_Size", &stored);
        assert_eq!(legacy, Some((Property::Unsigned(1282), None)));
        let stored = Value::Binary(vec![0xae, 0, 0, 0, 0, 0, 0, 0]);
        let store = decoder(Layout::PropertyStore).decode("System_Size", &stored);
        assert_eq!(store, Some((Property::Unsigned(174), None)));
    }

    #[test]
    fn filetimes_are_times() {
        let stored = Value::Binary(vec![0x01, 0xca, 0x04, 0x40, 0x1c, 0xc3, 0x5f, 0x15]);
        let (time, _) = decoder(Layout::Legacy)
            .decode("System_DateModified", &stored)
            .unwrap();
        let iso = time.as_time().and_then(|time| time.to_iso8601());
        assert_eq!(iso.as_deref(), Some("2009-07-14T05:01:14.0464405Z"));
    }

    #[test]
    fn binary_lists_split_at_nuls() {
        let stored: Vec<u8> = "link\0program"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        let (kind, _) = decoder(Layout::PropertyStore)
            .decode("System_Kind", &Value::Binary(stored))
            .unwrap();
        assert_eq!(
            kind.texts(),
            Some(vec!["link".to_owned(), "program".to_owned()])
        );
    }

    #[test]
    fn what_doesnt_decode_stays_bytes() {
        let decoder = decoder(Layout::Legacy);
        let short = Value::Binary(vec![1, 2, 3]);
        let (value, problem) = decoder.decode("System_Size", &short).unwrap();
        assert_eq!(value, Property::Bytes(vec![1, 2, 3]));
        assert!(problem.is_some());
        let unknown = decoder.decode("System_Unknown", &short).unwrap();
        assert_eq!(unknown, (Property::Bytes(vec![1, 2, 3]), None));
        assert_eq!(decoder.decode("System_Size", &Value::Null), None);
    }
}
