use zvte_comparison::{event_streams, verify, workloads, Event, Events};

fn equal_under_arbitrary_chunks(chunks: &[&[u8]]) {
    // Whole-input vte avoids its documented partial-UTF8 defects. Comparison
    // stays exact, and zvte still receives every requested chunk boundary.
    let input: Vec<u8> = chunks
        .iter()
        .flat_map(|chunk| chunk.iter().copied())
        .collect();
    let (actual, _) = event_streams::<1024>(chunks);
    let (_, expected) = event_streams::<1024>(&[&input]);
    assert_eq!(actual, expected, "chunks={chunks:02x?}");
}

fn equal<const N: usize>(chunks: &[&[u8]]) {
    let (actual, expected) = event_streams::<N>(chunks);
    assert_eq!(actual, expected, "input chunks: {chunks:02x?}");
}

#[test]
fn every_benchmark_has_identical_full_event_stream() {
    for workload in workloads() {
        verify(&workload.data, workload.chunk).unwrap_or_else(|e| panic!("{}: {e}", workload.name));
    }
}

#[test]
fn every_byte_from_every_reachable_control_state() {
    let prefixes: &[&[u8]] = &[
        b"",
        b"\x1b",
        b"\x1b ",
        b"\x1b[",
        b"\x1b[1;2",
        b"\x1b[1 ",
        b"\x1b[1?",
        b"\x1bP",
        b"\x1bP1;2",
        b"\x1bP1 ",
        b"\x1bP1?",
        b"\x1bPq",
        b"\x1b]",
        b"\x1b]0;title",
        b"\x1bX",
        b"\x1b^",
        b"\x1b_",
    ];
    for prefix in prefixes {
        for byte in 0..=255u8 {
            let mut input = prefix.to_vec();
            input.push(byte);
            input.extend_from_slice(b"\x18z\x1b[31m\x1bPqabc\x1b\\\x1b]0;t\x07");
            equal::<1024>(&[&input]);
            let chunks: Vec<_> = input.chunks(1).collect();
            equal::<1024>(&chunks);
        }
    }
}

#[test]
fn utf8_valid_invalid_and_truncated_all_boundaries() {
    let inputs: &[&[u8]] = &[
        "a🦀中文café🚀Ωz".as_bytes(),
        b"\xc2\x80\xc2\x9f\xc2\xa0",
        b"\x80\x9f\xa0\xbf\xc0\xc1\xf5\xff",
        b"\xe0\x80\x80\xed\xa0\x80\xf0\x80\x80\x80\xf4\x90\x80\x80",
        b"\xe2\x82x\xf0\x9f\x1b[31m!",
        b"\xf0\x9f\xa6",
        b"\xe6\x9c",
        b"\xc2",
        b"",
    ];
    for input in inputs {
        equal_under_arbitrary_chunks(&[input]);
        for first in 0..=input.len() {
            equal_under_arbitrary_chunks(&[&input[..first], b"", &input[first..]]);
            for second in first..=input.len() {
                equal_under_arbitrary_chunks(&[
                    &input[..first],
                    &input[first..second],
                    b"",
                    &input[second..],
                    b"\x18ok",
                ]);
            }
        }
    }
}

#[test]
fn utf8_boundary_prefixes_accept_or_reject_every_next_byte() {
    // These prefixes exercise each restricted second-byte range, both sides
    // of the surrogate gap, and each position in a four-byte sequence.
    let prefixes: &[&[u8]] = &[
        b"\xc2",
        b"\xdf",
        b"\xe0",
        b"\xe0\xa0",
        b"\xed",
        b"\xed\x9f",
        b"\xef\xbf",
        b"\xf0",
        b"\xf0\x90",
        b"\xf0\x90\x80",
        b"\xf4",
        b"\xf4\x8f",
        b"\xf4\x8f\xbf",
    ];
    for prefix in prefixes {
        for next in 0..=255 {
            let mut input = prefix.to_vec();
            input.push(next);
            input.extend_from_slice(b"Z\x1b[0m");
            for split in 0..=input.len() {
                equal_under_arbitrary_chunks(&[&input[..split], b"", &input[split..]]);
            }
        }
    }
}

#[test]
fn parameter_saturation_capacity_and_subparameters() {
    for &separators in b";:" {
        for count in [0usize, 1, 30, 31, 32, 33, 255, 4096] {
            let mut input = b"\x1b[?".to_vec();
            for _ in 0..count {
                input.extend_from_slice(b"99999999999999999999");
                input.push(separators);
            }
            input.extend_from_slice(b"12m\x1bP1:2;3$qbody\x1b\\");
            equal::<1024>(&[&input]);
            let chunks: Vec<_> = input.chunks(7).collect();
            equal::<1024>(&chunks);
        }
    }
    for input in [
        b"\x1b[1;:2;3::4m".as_slice(),
        b"\x1b[?1;2 !#$m",
        b"\x1b #$%A",
    ] {
        equal::<1024>(&[input]);
    }
}

#[test]
fn osc_limits_cancellation_and_all_terminators() {
    for capacity_case in 0..5 {
        let mut input = b"\x1b]0;".to_vec();
        input.extend(std::iter::repeat(b'x').take([0, 15, 1024, 1025, 4096][capacity_case]));
        input.extend_from_slice(b";a;b;c;d;e;f;g;h;i;j;k;l;m;n;o;p;q;r\x07");
        equal::<0>(&[&input]);
        equal::<1>(&[&input]);
        equal::<16>(&[&input]);
        equal::<1024>(&[&input]);
        equal::<4096>(&[&input]);
        for term in [b"\x07".as_slice(), b"\x18", b"\x1a", b"\x1b\\", b"\x9c"] {
            equal::<1024>(&[b"\x1b]2;title", term, b"\x1b[31mok\x1b[0m"]);
        }
    }
    equal::<1024>(&[b"\x1b];;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;\x07"]);
    equal::<1024>(&[b"\x1b]0;\0\x01\x06\x08\x19\x1f\x7f\x80\x9c\xff\x07"]);
}

#[test]
fn dcs_payload_cancellation_and_truncation() {
    let all: Vec<u8> = (0..=255u8).collect();
    for term in [b"\x18".as_slice(), b"\x1a", b"\x1b\\", b"\x9c", b""] {
        equal::<1024>(&[b"\x1bP1;2$qabc", term, b"\x1b[0mhello"]);
    }
    equal::<1024>(&[b"\x1bPq", &all]);
    equal::<1024>(&[b"\x1bP999999999999:12;3?discard", b"\x1b\\ok"]);
}

#[test]
fn truncated_prefixes_resume_without_lost_events() {
    let input =
        b"text\x1b[38:2::1:2:3m\x1b]8;id=1;https://example.com\x1b\\link\x1bP1;2$qpayload\x1b\\end";
    for split in 0..=input.len() {
        equal::<1024>(&[&input[..split]]);
        equal::<1024>(&[&input[..split], b"", &input[split..]]);
    }
}

fn random(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

#[test]
fn seeded_arbitrary_bytes_and_random_chunk_boundaries() {
    let mut seed = 0xd7c1_5a92_26e3_4b81;
    for _case in 0..4096 {
        let len = (random(&mut seed) as usize % 384) + 1;
        let mut input = Vec::with_capacity(len + 40);
        for _ in 0..len {
            input.push(random(&mut seed) as u8);
        }
        input.extend_from_slice(b"\x18\x1b[1:2;3m\x1b]0;after\x07\x1bPqend\x1b\\");
        let mut chunks = Vec::new();
        let mut offset = 0;
        while offset < input.len() {
            let end = (offset + (random(&mut seed) as usize % 19) + 1).min(input.len());
            chunks.push(&input[offset..end]);
            if random(&mut seed) & 3 == 0 {
                chunks.push(b"");
            }
            offset = end;
        }
        // The oracle gets the entire input because vte 0.15.0 has two
        // independently reproduced partial-UTF8 defects. zvte still receives
        // arbitrary chunks, including empty ones; comparison is exact.
        let (actual, _) = event_streams::<1024>(&chunks);
        let (whole, expected) = event_streams::<1024>(&[&input]);
        assert_eq!(actual, expected, "random chunk input={input:02x?}");
        assert_eq!(whole, expected, "whole input={input:02x?}");
    }
}

#[test]
fn reference_c1_chunk_boundary_behavior_is_explicit() {
    // vte 0.15.0 dispatches a decoded C1 as print when a partial UTF-8
    // codepoint crosses an advance call, and execute when it does not.
    // Keep the upstream behavior visible; never normalize event comparisons.
    let mut complete = vte::Parser::new();
    let mut split = vte::Parser::new();
    let mut complete_events = Events::default();
    let mut split_events = Events::default();
    complete.advance(&mut complete_events, b"\xc2\x80");
    split.advance(&mut split_events, b"\xc2");
    split.advance(&mut split_events, b"\x80");
    assert_eq!(complete_events.0, vec![Event::Execute(0x80)]);
    assert_eq!(split_events.0, vec![Event::Print('\u{80}')]);
    let (z_complete, _) = event_streams::<1024>(&[b"\xc2\x80"]);
    let (z_split, _) = event_streams::<1024>(&[b"\xc2", b"\x80"]);
    assert_eq!(z_complete, vec![Event::Execute(0x80)]);
    assert_eq!(z_split, z_complete);
}

#[test]
fn reference_drops_ascii_after_split_utf8_and_zvte_preserves_it() {
    // The reference's partial-UTF8 Err branch consumes valid_up_to bytes but
    // prints only its first character. A following ASCII space is lost.
    let input = b"\xc2\xa0 \xd0\xb0";
    let (whole, whole_reference) = event_streams::<1024>(&[input]);
    let (split, split_reference) = event_streams::<1024>(&[&input[..1], &input[1..]]);
    assert_eq!(
        whole_reference,
        vec![Event::Print('\u{a0}'), Event::Print(' '), Event::Print('а')]
    );
    assert_eq!(
        split_reference,
        vec![Event::Print('\u{a0}'), Event::Print('а')]
    );
    assert_eq!(split, whole);
    assert_eq!(split, whole_reference);
}
