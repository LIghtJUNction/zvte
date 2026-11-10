//! Safe word-at-a-time scanning. No alignment or target-feature preconditions.

/// Length of the leading printable ASCII run (DEL follows vte's print semantics).
#[inline]
pub(crate) fn ascii_run(bytes: &[u8]) -> usize {
    let mut i = 0;
    // One high bit per byte is set for bytes < 0x20 or >= 0x80. Masking before
    // subtraction prevents high-bit bytes from masking a preceding control.
    const HIGH: u64 = 0x8080_8080_8080_8080;
    const LOW: u64 = 0x7f7f_7f7f_7f7f_7f7f;
    const SPACE: u64 = 0x2020_2020_2020_2020;
    while bytes.len() - i >= 8 {
        let word = u64::from_ne_bytes(bytes[i..i + 8].try_into().unwrap());
        if ((word & LOW).wrapping_sub(SPACE) | word) & HIGH != 0 {
            break;
        }
        i += 8;
    }
    while i < bytes.len() && bytes[i].wrapping_sub(0x20) < 0x60 {
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_byte_at_every_offset() {
        for len in 0..40 {
            for pos in 0..len {
                for byte in 0..=255 {
                    let mut input = [b'A'; 40];
                    input[pos] = byte;
                    let expected = input[..len]
                        .iter()
                        .position(|&b| !(0x20..0x80).contains(&b))
                        .unwrap_or(len);
                    assert_eq!(ascii_run(&input[..len]), expected);
                }
            }
        }
    }
}
