#!/bin/sh
# Download SIDR's test indexes (Apache-2.0, too large to keep here) at a
# pinned commit, checking their SHA-256, into tests/fixtures/sidr/ for
# tests/sidr.rs: Windows.edb (a Windows 10 index, 32 MiB), Windows.db (a
# Windows 11 one, 2.8 MiB) and SIDR's reports of the latter, every value
# of which the test checks. Without them, those tests are skipped.
set -eu
cd "$(dirname "$0")/fixtures/sidr"
commit=5bde8e87b2f07f72c031eb9bfd39c50739a13aec
base="https://raw.githubusercontent.com/strozfriedberg/sidr/$commit/tests"
fetch() {
    if [ ! -f "$2" ]; then
        curl -sfL -o "$2" "$base/$1/$2"
    fi
}
fetch testdata Windows.edb
fetch testdata Windows.db
for report in File Activity_History Internet_History; do
    fetch goldenfiles "DESKTOP-O47KVAD_${report}_Report.json"
done
shasum -a 256 -c --quiet - <<SUMS
10dd5fc05c2d19aa1fa4a705142e413fc5a4af17ae8e5e4909164262f9de7c66  Windows.edb
e655a1af9eb3386ffdc7e19aa8dcda06dfa2c35a1c3b657ac4a9c1c13c83f020  Windows.db
f339a8e0a9bf0901c51faa70376a2554553e2b929fec304d00a216df3ec89bf5  DESKTOP-O47KVAD_File_Report.json
b5662459d05365f1f45da30354cd49e86a5d7bb73d688e080a976d573cb56659  DESKTOP-O47KVAD_Activity_History_Report.json
5c31d49e5a32a16337d231f02b5a67fd485111c6313459a107916a5d20025c4d  DESKTOP-O47KVAD_Internet_History_Report.json
SUMS
