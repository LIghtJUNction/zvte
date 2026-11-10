//! Safe native-word scanning: 64 bits on x86_64/aarch64, pointer width elsewhere.
//! No alignment or target-feature preconditions.

const WIDTH: usize = core::mem::size_of::<usize>();
const ONES: usize = usize::MAX / 255;
const HIGH: usize = ONES * 0x80;
const LOW: usize = ONES * 0x7f;

/// Length of the leading printable ASCII run (DEL follows vte's print semantics).
#[inline]
pub(crate) fn ascii_run(bytes: &[u8]) -> usize {
    let mut i = 0;
    // One high bit per byte is set for bytes < 0x20 or >= 0x80. Masking before
    // subtraction prevents high-bit bytes from masking a preceding control.
    const SPACE: usize = ONES * 0x20;
    while bytes.len() - i >= WIDTH {
        let word = usize::from_ne_bytes(bytes[i..i + WIDTH].try_into().unwrap());
        if ((word & LOW).wrapping_sub(SPACE) | word) & HIGH != 0 {
            break;
        }
        i += WIDTH;
    }
    while i < bytes.len() && bytes[i].wrapping_sub(0x20) < 0x60 {
        i += 1;
    }
    i
}

/// Find ESC in a byte stream, without inspecting or modifying UTF-8 boundaries.
#[inline]
pub(crate) fn escape(bytes: &[u8]) -> usize {
    let mut i = 0;
    while bytes.len() - i >= WIDTH {
        let word = usize::from_ne_bytes(bytes[i..i + WIDTH].try_into().unwrap()) ^ (ONES * 0x1b);
        if word.wrapping_sub(ONES) & !word & HIGH != 0 {
            break;
        }
        i += WIDTH;
    }
    while i < bytes.len() && bytes[i] != 0x1b {
        i += 1;
    }
    i
}

/// OSC bytes >= SPACE other than the field separator can be copied together.
#[inline]
pub(crate) fn osc_run(bytes: &[u8]) -> usize {
    let mut i = 0;
    while bytes.len() - i >= WIDTH {
        let word = usize::from_ne_bytes(bytes[i..i + WIDTH].try_into().unwrap());
        let separator = word ^ (ONES * 0x3b);
        let low = (word & LOW).wrapping_sub(ONES * 0x20) & !word;
        let semicolon = separator.wrapping_sub(ONES) & !separator;
        if (low | semicolon) & HIGH != 0 {
            break;
        }
        i += WIDTH;
    }
    while i < bytes.len() && bytes[i] >= 0x20 && bytes[i] != b';' {
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
            if cfg!(miri) && ![0, 1, 7, 8, 9, 15, 16, 17, 31, 32, 39].contains(&len) {
                continue;
            }
            for pos in 0..len {
                for byte in 0..=255 {
                    if cfg!(miri)
                        && ![0, 7, 27, 31, 32, 65, 126, 127, 128, 159, 160, 255].contains(&byte)
                    {
                        continue;
                    }
                    let mut input = [b'A'; 40];
                    input[pos] = byte;
                    let expected = input[..len]
                        .iter()
                        .position(|&b| !(0x20..0x80).contains(&b))
                        .unwrap_or(len);
                    assert_eq!(ascii_run(&input[..len]), expected);
                    let expected_esc = input[..len].iter().position(|&b| b == 0x1b).unwrap_or(len);
                    assert_eq!(escape(&input[..len]), expected_esc);
                    let expected_osc = input[..len]
                        .iter()
                        .position(|&b| b < 0x20 || b == b';')
                        .unwrap_or(len);
                    assert_eq!(osc_run(&input[..len]), expected_osc);
                }
            }
        }
    }
}
