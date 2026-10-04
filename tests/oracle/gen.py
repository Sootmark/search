#!/usr/bin/env python3
r"""Writes esedbexport's reading of a Windows Search property table as the
oracle tests/oracle.rs compares with. Run by gen.sh, which says how.

    gen.py EXPORTED_TABLE > ORACLE

EXPORTED_TABLE is esedbexport's export of SystemIndex_0A. The first line
names the columns, tab-separated; then one line per item, in table order:
DocID, then each property as esedbexport prints it, but for:

    -          no value (esedbexport prints nothing)
    times      ISO 8601 UTC to the 100 ns (esedbexport: "Jul 14, 2009
               05:01:14.046440500")
    text       as esedbexport writes it: backslashes doubled, line feeds
               as \n

A NULL fixed value esedbexport reads from the record's filler bytes (the
0x2a ESE writes: System_Size "********", System_FileAttributes 707406378)
is kept as printed.
"""

import csv
import sys
from datetime import datetime

PROPERTIES = [
    "System_ItemPathDisplay",
    "System_ItemFolderPathDisplay",
    "System_FileName",
    "System_ItemUrl",
    "System_ItemType",
    "System_ItemTypeText",
    "System_Kind",
    "System_Size",
    "System_FileAttributes",
    "System_DateModified",
    "System_DateCreated",
    "System_DateAccessed",
    "System_Search_GatherTime",
    "System_FileOwner",
    "System_ComputerName",
    "System_Title",
    "Microsoft_IE_Title",
    "System_Search_AutoSummary",
]
TIMES = {
    "System_DateModified",
    "System_DateCreated",
    "System_DateAccessed",
    "System_Search_GatherTime",
}


def iso(text):
    """esedbexport's "Mon DD, YYYY HH:MM:SS.nnnnnnnnn" as ISO 8601."""
    stamp, nanos = text.rsplit(".", 1)
    when = datetime.strptime(stamp, "%b %d, %Y %H:%M:%S")
    return when.strftime("%Y-%m-%dT%H:%M:%S") + "." + nanos[:7] + "Z"


def cell(name, text):
    if text == "":
        return "-"
    if name in TIMES:
        return iso(text)
    return text


def main():
    csv.field_size_limit(sys.maxsize)
    with open(sys.argv[1], newline="", encoding="utf-8", errors="surrogateescape") as table:
        rows = csv.reader(table, delimiter="\t", quoting=csv.QUOTE_NONE)
        header = next(rows)
        columns = [header.index(name) for name in ["DocID"] + PROPERTIES]
        print("\t".join(["DocID"] + PROPERTIES))
        for row in rows:
            names = ["DocID"] + PROPERTIES
            print("\t".join(cell(name, row[at]) for name, at in zip(names, columns)))


if __name__ == "__main__":
    main()
