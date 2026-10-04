#!/bin/sh
# Recreates tests/oracle/Windows.edb.tsv.gz: libesedb's esedbexport's
# reading of plaso's Windows.edb property table, the properties this crate
# interprets, compared with this crate's by tests/oracle.rs.
#
# esedbexport knows Windows Search's binary values: the big-endian
# FILETIMEs and sizes of SystemIndex_0A, and System_Search_AutoSummary
# compressed and obfuscated. libesedb (LGPL) is only run here, never
# vendored.
#
# Run on a Linux machine (made with libesedb 20240420 on Debian 13):
#   sudo apt-get install libesedb-utils
#   sh tests/oracle/gen.sh
set -eu

here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

gzip -dc "$here/../fixtures/plaso/Windows.edb.gz" > "$work/Windows.edb"
esedbexport -m tables -T SystemIndex_0A -t "$work/Windows" "$work/Windows.edb" > /dev/null
python3 "$here/gen.py" "$work"/Windows.export/SystemIndex_0A.* > "$work/Windows.edb.tsv"
gzip -9nc "$work/Windows.edb.tsv" > "$here/Windows.edb.tsv.gz"
