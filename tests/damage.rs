//! Any input gives items, problems or an error, never a panic.

mod support;

use proptest::prelude::*;

/// The database's page size.
const PAGE: usize = 32 * 1024;

/// The offsets of the pages that hold something: 101 of 1,282, so that
/// damage lands where it matters.
fn used_pages(data: &[u8]) -> Vec<usize> {
    (0..data.len() / PAGE)
        .map(|page| page * PAGE)
        .filter(|&at| data[at..at + PAGE].iter().any(|&byte| byte != 0))
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn arbitrary_bytes(data in proptest::collection::vec(any::<u8>(), 0..70_000)) {
        let _ = search::read(&data);
    }

    #[test]
    fn the_database_damaged(flips in proptest::collection::vec((any::<usize>(), any::<u8>()), 1..200)) {
        let mut data = support::windows_edb();
        let pages = used_pages(&data);
        for (at, byte) in flips {
            let page = pages[at % pages.len()];
            data[page + at / pages.len() % PAGE] = byte;
        }
        let _ = search::read(&data);
    }

    #[test]
    fn the_database_cut_short(keep in 0usize..42_008_576) {
        let data = support::windows_edb();
        let _ = search::read(&data[..keep.min(data.len())]);
    }
}

#[test]
fn not_an_index() {
    assert!(search::read(b"").is_err());
}
