//! Windows Search's own encoding of text it keeps as binary: in Windows
//! Vista and 7 (`SystemIndex_0A`), `System_Search_AutoSummary` is
//! compressed, then lightly obfuscated, before ESE stores it.
//!
//! The obfuscation is documented by Joachim Metz, "Windows Search
//! forensics" (2010), §2.1 Data obfuscation: each byte is XOR-ed with a
//! byte of the 32-bit mask `0x05000113` XOR the data's size, the mask byte
//! chosen by the byte's position modulo 4, then with the low byte of its
//! position. The mask is the XOR of the 32-bit words of a binary SID; the
//! paper names it `S-1-5-12`, but the bytes it gives
//! (`01 01 00 00 00 00 00 05 12 00 00 00`) are those of `S-1-5-18`.
//!
//! The compression (Metz, §2.2, describes it only as "multiple compression
//! methods" behind `MSSUncompressText`) was worked out on plaso's
//! `Windows.edb` and checked against libesedb's `esedbexport` on every value
//! it holds (`tests/oracle/`). Its first byte says how the UTF-16 text is
//! stored:
//!
//! - `0x01`: one byte per character, every high byte zero (ISO 8859-1);
//! - `0x00`: runs of characters sharing a high byte, each a count, the high
//!   byte, then the count's low bytes (`1f 00 "A chicken…"`, `01 20 19` for
//!   U+2019).
//!
//! Other first bytes may exist, but no openly licensed database holds one:
//! they are reported and the value kept as bytes.

/// The XOR of the little-endian 32-bit words of the binary SID
/// `01 01 00 00 00 00 00 05 12 00 00 00` (`S-1-5-18`).
const MASK: u32 = 0x0500_0113;
/// First byte of text stored one byte per character.
const BYTES: u8 = 0x01;
/// First byte of text stored as runs sharing a high byte.
const RUNS: u8 = 0x00;

/// Undo the obfuscation (Metz 2010, §2.1). It is its own inverse.
pub(crate) fn deobfuscate(data: &[u8]) -> Vec<u8> {
    let mask = MASK ^ data.len() as u32;
    data.iter()
        .enumerate()
        .map(|(at, &byte)| {
            let mask_byte = (mask >> (8 * (at % 4))) as u8;
            byte ^ mask_byte ^ at as u8
        })
        .collect()
}

/// The text of an obfuscated, compressed value.
pub(crate) fn decode(data: &[u8]) -> Result<String, String> {
    let plain = deobfuscate(data);
    let (&method, body) = plain.split_first().ok_or("an empty encoded value")?;
    let units = match method {
        BYTES => body.iter().map(|&low| u16::from(low)).collect(),
        RUNS => runs(body)?,
        other => return Err(format!("text compression {other:#04x} isn't supported")),
    };
    Ok(crate::property::text(&units))
}

/// Runs of a count, a high byte and the count's low bytes.
fn runs(mut body: &[u8]) -> Result<Vec<u16>, String> {
    let mut units = Vec::with_capacity(body.len());
    while let Some((&count, rest)) = body.split_first() {
        let (&high, rest) = rest
            .split_first()
            .ok_or("a run of compressed text without its high byte")?;
        let lows = rest
            .get(..usize::from(count))
            .ok_or_else(|| format!("a run of {count} characters cut short"))?;
        units.extend(lows.iter().map(|&low| u16::from_be_bytes([high, low])));
        body = &rest[lows.len()..];
    }
    Ok(units)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mask_is_the_sids_bytes_xored() {
        let sid = [1u8, 1, 0, 0, 0, 0, 0, 5, 0x12, 0, 0, 0];
        let mask = sid.chunks_exact(4).fold(0, |mask, word| {
            mask ^ u32::from_le_bytes([word[0], word[1], word[2], word[3]])
        });
        assert_eq!(mask, MASK);
    }

    #[test]
    fn deobfuscation_is_its_own_inverse() {
        let data: Vec<u8> = (0..=255).collect();
        assert_eq!(deobfuscate(&deobfuscate(&data)), data);
    }

    #[test]
    fn a_value_from_plaso_s_windows_edb() {
        // DocID 16's System_Search_AutoSummary, as stored.
        let stored = [
            0x32, 0x42, 0x77, 0x74, 0x59, 0x24, 0x76, 0x6b, 0x58, 0x7c, 0x7f, 0x7c, 0x5a, 0x7f,
            0x2e, 0x6b, 0x4d, 0x74, 0x32, 0x60, 0x4e, 0x70, 0x73, 0x7d, 0x0b, 0x6c, 0x75, 0x3e,
            0x6b, 0x4a, 0x5a, 0x34,
        ];
        assert_eq!(
            decode(&stored).as_deref(),
            Ok("Burn pictures and video to DVD.")
        );
    }

    #[test]
    fn a_value_in_runs_from_plaso_s_windows_edb() {
        // DocID 229's System_Search_AutoSummary, as stored: three runs.
        let stored = [
            0x3c, 0x1f, 0x02, 0x47, 0x18, 0x67, 0x6e, 0x6b, 0x57, 0x63, 0x6f, 0x60, 0x10, 0x65,
            0x7d, 0x2a, 0x4d, 0x30, 0x66, 0x6f, 0x58, 0x71, 0x36, 0x7d, 0x42, 0x38, 0x78, 0x77,
            0x52, 0x78, 0x30, 0x3a, 0x55, 0x54, 0x23, 0x06, 0x01, 0x2c, 0x26, 0x51, 0x34, 0x5c,
            0x4b, 0x5d, 0x64, 0x55, 0x00,
        ];
        assert_eq!(
            decode(&stored).as_deref(),
            Ok("A chicken is a type of bird. It\u{2019}s tasty.")
        );
    }

    /// `method` then `body`, obfuscated as Windows Search stores them.
    fn encoded(method: u8, body: &[u8]) -> Vec<u8> {
        let mut plain = vec![method];
        plain.extend_from_slice(body);
        deobfuscate(&plain)
    }

    #[test]
    fn runs_carry_their_high_byte() {
        let body = [2, 0, b'I', b't', 1, 0x20, 0x19, 1, 0, b's'];
        assert_eq!(decode(&encoded(RUNS, &body)).as_deref(), Ok("It\u{2019}s"));
    }

    #[test]
    fn single_bytes_are_latin_1() {
        assert_eq!(
            decode(&encoded(BYTES, b"caf\xe9\0")).as_deref(),
            Ok("caf\u{e9}")
        );
    }

    #[test]
    fn damage_is_an_error() {
        assert!(decode(&[]).is_err());
        assert!(decode(&encoded(RUNS, &[5, 0, b'a'])).is_err());
        assert!(decode(&encoded(RUNS, &[5])).is_err());
        let unknown = decode(&encoded(0x07, b"x")).unwrap_err();
        assert_eq!(unknown, "text compression 0x07 isn't supported");
    }
}
