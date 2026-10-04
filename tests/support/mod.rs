//! The test database, plaso's `Windows.edb` (Apache-2.0,
//! `tests/fixtures/plaso/`, stored gzip-compressed).

/// The database's bytes, decompressed.
pub fn windows_edb() -> Vec<u8> {
    let compressed = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/plaso/Windows.edb.gz"
    ))
    .unwrap();
    let mut data = Vec::new();
    std::io::Read::read_to_end(
        &mut common::gzip::Decoder::new(compressed.as_slice()),
        &mut data,
    )
    .unwrap();
    data
}
