use zvte::{Params, Parser, Perform};

#[test]
fn long_invalid_utf8_run_preserves_every_scalar() {
    #[derive(Default)]
    struct Counts {
        nbsp: usize,
        replacement: usize,
    }
    impl Perform for Counts {
        fn print(&mut self, c: char) {
            match c {
                '\u{a0}' => self.nbsp += 1,
                '\u{fffd}' => self.replacement += 1,
                _ => panic!("unexpected scalar {c:?}"),
            }
        }
    }
    let repetitions = if cfg!(miri) { 64 } else { 65_536 };
    let bytes = [0xc2, 0xa0, 0xff].repeat(repetitions);
    let mut output = Counts::default();
    let mut parser = Parser::new();
    parser.advance(&bytes, &mut output);
    assert_eq!(
        (output.nbsp, output.replacement),
        (repetitions, repetitions)
    );
    assert!(parser.is_ground());
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Event {
    Print(char),
    Execute(u8),
    Esc(Vec<u8>, bool, u8),
    Csi(Vec<Vec<u16>>, Vec<u8>, bool, char),
    Osc(Vec<Vec<u8>>, bool),
    Hook(Vec<Vec<u16>>, Vec<u8>, bool, char),
    Put(u8),
    Unhook,
}

#[derive(Default)]
struct Recorder(Vec<Event>);

impl Perform for Recorder {
    fn print(&mut self, c: char) {
        self.0.push(Event::Print(c));
    }
    fn execute(&mut self, byte: u8) {
        self.0.push(Event::Execute(byte));
    }
    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        self.0
            .push(Event::Esc(intermediates.to_vec(), ignore, byte));
    }
    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
        self.0.push(Event::Csi(
            params.iter().map(<[u16]>::to_vec).collect(),
            intermediates.to_vec(),
            ignore,
            action,
        ));
    }
    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        self.0.push(Event::Osc(
            params.iter().map(|p| p.to_vec()).collect(),
            bell_terminated,
        ));
    }
    fn hook(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
        self.0.push(Event::Hook(
            params.iter().map(<[u16]>::to_vec).collect(),
            intermediates.to_vec(),
            ignore,
            action,
        ));
    }
    fn put(&mut self, byte: u8) {
        self.0.push(Event::Put(byte));
    }
    fn unhook(&mut self) {
        self.0.push(Event::Unhook);
    }
}

fn parse(input: &[u8]) -> Vec<Event> {
    let mut parser = Parser::new();
    let mut recorder = Recorder::default();
    parser.advance(input, &mut recorder);
    recorder.0
}

fn csi(params: &[&[u16]], intermediates: &[u8], ignore: bool, action: char) -> Event {
    Event::Csi(
        params.iter().map(|p| p.to_vec()).collect(),
        intermediates.to_vec(),
        ignore,
        action,
    )
}

#[test]
fn ground_reports_printable_text_c0_c1_and_del() {
    assert_eq!(
        parse(b"A\0\t\n\r\x1f\x7f\x80\x9f\xc2\x85\xe4\xb8\xad\xf0\x9f\xa6\x80"),
        vec![
            Event::Print('A'),
            Event::Execute(0),
            Event::Execute(9),
            Event::Execute(10),
            Event::Execute(13),
            Event::Execute(31),
            Event::Print('\x7f'),
            Event::Execute(0x80),
            Event::Execute(0x9f),
            Event::Execute(0x85),
            Event::Print('中'),
            Event::Print('🦀'),
        ]
    );
}

#[test]
fn csi_private_markers_subparameters_and_defaults() {
    assert_eq!(
        parse(b"\x1b[m\x1b[;m\x1b[38:2::255:0:42m\x1b[?25h\x1b[>1;2c\x1b[12 $p"),
        vec![
            csi(&[&[0]], b"", false, 'm'),
            csi(&[&[0], &[0]], b"", false, 'm'),
            csi(&[&[38, 2, 0, 255, 0, 42]], b"", false, 'm'),
            csi(&[&[25]], b"?", false, 'h'),
            csi(&[&[1], &[2]], b">", false, 'c'),
            csi(&[&[12]], b" $", false, 'p'),
        ]
    );
}

#[test]
fn csi_executes_embedded_controls_without_losing_parameters() {
    assert_eq!(
        parse(b"\x1b[1\x00;\n2m"),
        vec![
            Event::Execute(0),
            Event::Execute(10),
            csi(&[&[1], &[2]], b"", false, 'm'),
        ]
    );
}

#[test]
fn numeric_overflow_saturates_and_keeps_the_next_parameter() {
    assert_eq!(
        parse(b"\x1b[65535;65536;999999999999999999999999;7m"),
        vec![csi(&[&[65535], &[65535], &[65535], &[7]], b"", false, 'm')]
    );
}

#[test]
fn parameter_capacity_is_bounded_and_reported() {
    let mut input = b"\x1b[".to_vec();
    for i in 0..40 {
        if i != 0 {
            input.push(b';');
        }
        input.extend_from_slice(i.to_string().as_bytes());
    }
    input.push(b'm');
    let expected = (0..32).map(|i| vec![i]).collect();
    assert_eq!(parse(&input), vec![Event::Csi(expected, vec![], true, 'm')]);

    let mut input = b"\x1b[".to_vec();
    input.extend_from_slice(&[b':'; 80]);
    input.push(b'm');
    assert_eq!(
        parse(&input),
        vec![Event::Csi(vec![vec![0; 32]], vec![], true, 'm')]
    );
}

#[test]
fn exactly_full_parameters_are_not_marked_ignored() {
    let mut input = b"\x1b[".to_vec();
    input.extend_from_slice(&[b';'; 31]);
    input.push(b'm');
    assert_eq!(
        parse(&input),
        vec![Event::Csi(vec![vec![0]; 32], vec![], false, 'm')]
    );
}

#[test]
fn extra_intermediates_preserve_the_prefix_and_set_ignore() {
    assert_eq!(
        parse(b"\x1b[1 !\"p\x1b !\"0"),
        vec![
            csi(&[&[1]], b" !", true, 'p'),
            Event::Esc(b" !".to_vec(), true, b'0'),
        ]
    );
}

#[test]
fn malformed_parameter_order_is_discarded_and_recovers() {
    assert_eq!(
        parse(b"\x1b[1?2mA\x1b[ 4mB\x1b[1?\n2mC"),
        vec![
            Event::Print('A'),
            Event::Print('B'),
            Event::Execute(10),
            Event::Print('C'),
        ]
    );
}

#[test]
fn escape_dispatch_and_control_interruption() {
    assert_eq!(
        parse(b"\x1b7\x1b(B\x1b#8\x1b\nD\x1b[12\x1b(A\x1b[1\x18Z\x1b[2\x1aY"),
        vec![
            Event::Esc(vec![], false, b'7'),
            Event::Esc(b"(".to_vec(), false, b'B'),
            Event::Esc(b"#".to_vec(), false, b'8'),
            Event::Execute(10),
            Event::Esc(vec![], false, b'D'),
            Event::Esc(b"(".to_vec(), false, b'A'),
            Event::Execute(0x18),
            Event::Print('Z'),
            Event::Execute(0x1a),
            Event::Print('Y'),
        ]
    );
}

#[test]
fn osc_terminators_and_empty_fields_keep_callback_order() {
    assert_eq!(
        parse(b"\x1b]0;hello\x07\x1b]8;;url\x1b\\\x1b];\x07\x1b]\x07"),
        vec![
            Event::Osc(vec![b"0".to_vec(), b"hello".to_vec()], true),
            Event::Osc(vec![b"8".to_vec(), vec![], b"url".to_vec()], false),
            Event::Esc(vec![], false, b'\\'),
            Event::Osc(vec![vec![], vec![]], true),
            Event::Osc(vec![vec![]], true),
        ]
    );
}

#[test]
fn osc_cancel_finishes_before_execute() {
    assert_eq!(
        parse(b"\x1b]2;one\x18A\x1b]2;two\x1aB"),
        vec![
            Event::Osc(vec![b"2".to_vec(), b"one".to_vec()], false),
            Event::Execute(0x18),
            Event::Print('A'),
            Event::Osc(vec![b"2".to_vec(), b"two".to_vec()], false),
            Event::Execute(0x1a),
            Event::Print('B'),
        ]
    );
}

#[test]
fn osc_preserves_arbitrary_bytes_and_ignores_nonterminating_c0() {
    assert_eq!(
        parse(b"\x1b]2;\0a\t\n\r\x7f\x9c\xff\x07"),
        vec![Event::Osc(
            vec![b"2".to_vec(), b"a\x7f\x9c\xff".to_vec()],
            true
        )]
    );
}

#[test]
fn osc_capacity_counts_payload_not_separators() {
    let mut parser = Parser::<5>::new_with_size();
    let mut recorder = Recorder::default();
    parser.advance(b"\x1b]2;abcde;ignored\x07X", &mut recorder);
    assert_eq!(
        recorder.0,
        vec![
            Event::Osc(vec![b"2".to_vec(), b"abcd".to_vec()], true),
            Event::Print('X'),
        ]
    );
}

#[test]
fn zero_capacity_osc_is_safe_and_resynchronizes() {
    let mut parser = Parser::<0>::new_with_size();
    let mut recorder = Recorder::default();
    parser.advance(b"\x1b]0;ignored\x1b\\A", &mut recorder);
    assert_eq!(
        recorder.0,
        vec![
            Event::Osc(vec![vec![]], false),
            Event::Esc(vec![], false, b'\\'),
            Event::Print('A'),
        ]
    );
}

#[test]
fn osc_fields_are_limited_to_sixteen() {
    let mut input = b"\x1b]".to_vec();
    for _ in 0..40 {
        input.extend_from_slice(b"a;");
    }
    input.push(7);
    assert_eq!(
        parse(&input),
        vec![Event::Osc(vec![b"a".to_vec(); 16], true)]
    );
}

#[test]
fn dcs_dispatches_parameters_payload_and_terminator() {
    assert_eq!(
        parse(b"\x1bP1;2:3$qA\n\0\x7f\x80\xffB\x1b\\Z"),
        vec![
            Event::Hook(vec![vec![1], vec![2, 3]], b"$".to_vec(), false, 'q'),
            Event::Put(b'A'),
            Event::Put(10),
            Event::Put(0),
            Event::Put(b'B'),
            Event::Unhook,
            Event::Esc(vec![], false, b'\\'),
            Event::Print('Z'),
        ]
    );
}

#[test]
fn dcs_header_ignores_c0_and_all_supported_terminators_unhook() {
    assert_eq!(
        parse(b"\x1bP1\nqA\x18\x1bPqB\x1a\x1bPqC\x9cD"),
        vec![
            Event::Hook(vec![vec![1]], vec![], false, 'q'),
            Event::Put(b'A'),
            Event::Unhook,
            Event::Execute(0x18),
            Event::Hook(vec![vec![0]], vec![], false, 'q'),
            Event::Put(b'B'),
            Event::Unhook,
            Event::Execute(0x1a),
            Event::Hook(vec![vec![0]], vec![], false, 'q'),
            Event::Put(b'C'),
            Event::Unhook,
            Event::Print('D'),
        ]
    );
}

#[test]
fn malformed_dcs_and_sos_pm_apc_consume_strings_until_escape() {
    assert_eq!(
        parse(b"\x1bP1?ignored\x1b\\A\x1bXignored\x1b\\B\x1b^ignored\x18C\x1b_ignored\x1aD"),
        vec![
            Event::Esc(vec![], false, b'\\'),
            Event::Print('A'),
            Event::Esc(vec![], false, b'\\'),
            Event::Print('B'),
            Event::Execute(0x18),
            Event::Print('C'),
            Event::Execute(0x1a),
            Event::Print('D'),
        ]
    );
}

#[test]
fn invalid_utf8_replaces_sequences_without_losing_control_bytes() {
    assert_eq!(
        parse(b"a\xe2\x82X\xc0\xaf\xed\xa0\x80\xd8\x1b[31mZ"),
        vec![
            Event::Print('a'),
            Event::Print('\u{fffd}'),
            Event::Print('X'),
            Event::Print('\u{fffd}'),
            Event::Print('\u{fffd}'),
            Event::Print('\u{fffd}'),
            Event::Print('\u{fffd}'),
            Event::Execute(0x80),
            Event::Print('\u{fffd}'),
            csi(&[&[31]], b"", false, 'm'),
            Event::Print('Z'),
        ]
    );
}

#[test]
fn utf8_waits_for_completion_across_empty_and_single_byte_chunks() {
    let mut parser = Parser::new();
    let mut recorder = Recorder::default();
    for (index, byte) in "🦀".as_bytes().iter().enumerate() {
        parser.advance(&[*byte], &mut recorder);
        parser.advance(&[], &mut recorder);
        if index < 3 {
            assert!(recorder.0.is_empty());
        }
    }
    assert_eq!(recorder.0, vec![Event::Print('🦀')]);
    assert!(parser.is_ground());
}

#[test]
fn finish_flushes_incomplete_utf8_and_is_idempotent() {
    let mut parser = Parser::new();
    let mut recorder = Recorder::default();
    parser.advance(b"A\xf0\x9f", &mut recorder);
    assert_eq!(recorder.0, vec![Event::Print('A')]);
    parser.finish(&mut recorder);
    parser.finish(&mut recorder);
    assert_eq!(
        recorder.0,
        vec![Event::Print('A'), Event::Print('\u{fffd}')]
    );
    assert!(parser.is_ground());
    parser.advance(b"Z", &mut recorder);
    assert_eq!(recorder.0.last(), Some(&Event::Print('Z')));
}

#[test]
fn truncated_control_sequences_wait_without_dispatching() {
    for input in [b"\x1b".as_slice(), b"\x1b[12;", b"\x1b]2;title", b"\x1bP1;"] {
        let mut parser = Parser::new();
        let mut recorder = Recorder::default();
        parser.advance(input, &mut recorder);
        parser.advance(&[], &mut recorder);
        assert!(recorder.0.is_empty(), "input={input:?}");
        assert!(!parser.is_ground());
        parser.finish(&mut recorder);
        assert!(recorder.0.is_empty());
        assert!(parser.is_ground());
    }
}

#[test]
fn reset_abandons_partial_state_without_replaying_bytes() {
    let mut parser = Parser::new();
    let mut recorder = Recorder::default();
    for input in [b"\xe2\x82".as_slice(), b"\x1b[38;", b"\x1b]2;", b"\x1bP1;"] {
        parser.advance(input, &mut recorder);
        parser.reset();
        assert!(parser.is_ground());
        parser.advance(b"A", &mut recorder);
    }
    assert_eq!(recorder.0, vec![Event::Print('A'); 4]);
}

#[test]
fn every_split_preserves_events_for_all_sequence_families() {
    let input = b"plain\t\xc4\xb8\xf0\x9f\xa6\x80\x1b[38:2::1:2:3mcolor\x1b[m\x1b]8;;https://example.test\x1b\\link\x1b]8;;\x07\x1bP1;2$qpayload\x1b\\\x1b(B\x1bXignored\x1b\\\xe2\x82X\x1b[12;";
    let expected = parse(input);
    for split in 0..=input.len() {
        let mut parser = Parser::new();
        let mut recorder = Recorder::default();
        parser.advance(&input[..split], &mut recorder);
        parser.advance(&[], &mut recorder);
        parser.advance(&input[split..], &mut recorder);
        assert_eq!(recorder.0, expected, "split={split}");
    }
}

#[test]
fn batched_text_has_identical_events_and_excludes_controls() {
    #[derive(Default)]
    struct Batched {
        recorder: Recorder,
        calls: usize,
    }
    impl Perform for Batched {
        fn text(&mut self, text: &[u8]) {
            assert!(!text.is_empty());
            assert!(text.iter().all(|&byte| (0x20..=0x7f).contains(&byte)));
            self.calls += 1;
            for &byte in text {
                self.recorder.print(byte as char);
            }
        }
        fn print(&mut self, c: char) {
            self.recorder.print(c);
        }
        fn execute(&mut self, byte: u8) {
            self.recorder.execute(byte);
        }
    }
    let input = "a long plain ASCII span 中🦀\nsecond plain span\u{85}final span";
    let expected = parse(input.as_bytes());
    let mut handler = Batched::default();
    Parser::new().advance(input.as_bytes(), &mut handler);
    assert_eq!(handler.recorder.0, expected);
    assert!(handler.calls > 0);
    assert!(handler.calls < input.chars().count() / 2);
}

#[test]
fn randomized_chunks_preserve_events_and_make_progress() {
    let mut seed = 0x5639_6173_f861_c3d1u64;
    let mut random = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let cases = if cfg!(miri) { 8 } else { 1024 };
    for case in 0..cases {
        let len = (random() as usize % 512) + 1;
        let mut input = vec![0; len];
        for byte in &mut input {
            *byte = random() as u8;
        }
        let mut whole = Parser::<17>::new_with_size();
        let mut expected = Recorder::default();
        whole.advance(&input, &mut expected);
        whole.finish(&mut expected);
        let mut chunked = Parser::<17>::new_with_size();
        let mut actual = Recorder::default();
        let mut offset = 0;
        while offset < input.len() {
            let end = (offset + (random() as usize % 23) + 1).min(input.len());
            chunked.advance(&input[offset..end], &mut actual);
            chunked.advance(&[], &mut actual);
            offset = end;
        }
        chunked.finish(&mut actual);
        assert_eq!(actual.0, expected.0, "case={case}, input={input:?}");
        assert!(chunked.is_ground());
    }
}

#[test]
fn utf8_encoded_c1_is_a_control_for_every_split() {
    for byte in 0x80..=0x9f {
        let input = [0xc2, byte];
        for split in 0..=2 {
            let mut parser = Parser::new();
            let mut recorder = Recorder::default();
            parser.advance(&input[..split], &mut recorder);
            parser.advance(&input[split..], &mut recorder);
            assert_eq!(
                recorder.0,
                vec![Event::Execute(byte)],
                "byte={byte:#x}, split={split}"
            );
        }
    }
}

#[test]
fn finish_unhooks_active_dcs_once() {
    let mut parser = Parser::new();
    let mut recorder = Recorder::default();
    parser.advance(b"\x1bPqA", &mut recorder);
    parser.finish(&mut recorder);
    parser.finish(&mut recorder);
    assert_eq!(
        recorder.0,
        vec![
            Event::Hook(vec![vec![0]], vec![], false, 'q'),
            Event::Put(b'A'),
            Event::Unhook,
        ]
    );
    assert!(parser.is_ground());
}

#[test]
fn reset_discards_dcs_without_sending_unhook() {
    let mut parser = Parser::new();
    let mut recorder = Recorder::default();
    parser.advance(b"\x1bPq", &mut recorder);
    parser.reset();
    parser.finish(&mut recorder);
    parser.advance(b"A", &mut recorder);
    assert_eq!(
        recorder.0,
        vec![
            Event::Hook(vec![vec![0]], vec![], false, 'q'),
            Event::Print('A'),
        ]
    );
}

#[test]
fn parameter_iteration_reports_total_values_and_terminates() {
    struct Inspector;
    impl Perform for Inspector {
        fn csi_dispatch(&mut self, params: &Params, _: &[u8], _: bool, _: char) {
            assert!(!params.is_empty());
            assert_eq!(params.len(), 6);
            assert_eq!(
                params.iter().collect::<Vec<_>>(),
                vec![&[1, 2, 3][..], &[4][..], &[5, 6][..]]
            );
            assert_eq!(params.into_iter().count(), 3);
            assert!(!format!("{params:?}").is_empty());
            let mut iter = params.iter();
            for _ in 0..3 {
                assert!(iter.next().is_some());
            }
            assert!(iter.next().is_none());
            assert!(iter.next().is_none());
        }
    }
    Parser::new().advance(b"\x1b[1:2:3;4;5:6m", &mut Inspector);
}

#[test]
fn parser_instances_keep_independent_partial_sequences() {
    let mut left = Parser::new();
    let mut right = Parser::new();
    let mut left_events = Recorder::default();
    let mut right_events = Recorder::default();
    left.advance(b"\x1b[38:2::1:", &mut left_events);
    right.advance(b"\x1b]2;other", &mut right_events);
    left.advance(b"2:3m", &mut left_events);
    right.advance(b" session\x07", &mut right_events);
    assert_eq!(
        left_events.0,
        vec![csi(&[&[38, 2, 0, 1, 2, 3]], b"", false, 'm')]
    );
    assert_eq!(
        right_events.0,
        vec![Event::Osc(
            vec![b"2".to_vec(), b"other session".to_vec()],
            true
        )]
    );
}

#[test]
fn every_byte_in_each_state_can_resynchronize() {
    let prefixes: &[&[u8]] = &[
        b"",
        b"\x1b",
        b"\x1b(",
        b"\x1b[",
        b"\x1b[12;",
        b"\x1b[ ",
        b"\x1b[1?",
        b"\x1bP",
        b"\x1bP12;",
        b"\x1bP ",
        b"\x1bP1?",
        b"\x1bPq",
        b"\x1b]2;",
        b"\x1bX",
        b"\xc2",
        b"\xe2\x82",
        b"\xf0\x9f\xa6",
    ];
    for prefix in prefixes {
        for byte in 0..=255 {
            // Keep interpreter runs small while retaining every byte natively.
            if cfg!(miri) && ![0, 7, 24, 26, 27, 59, 127, 128, 156, 255].contains(&byte) {
                continue;
            }
            let mut input = prefix.to_vec();
            input.push(byte);
            input.extend_from_slice(b"\x1b\\Z");
            let expected = parse(&input);
            let mut parser = Parser::<1024>::new_with_size();
            let mut recorder = Recorder::default();
            parser.advance(prefix, &mut recorder);
            parser.advance(&[byte], &mut recorder);
            parser.advance(b"\x1b\\Z", &mut recorder);
            assert_eq!(recorder.0, expected, "prefix={prefix:?}, byte={byte:#x}");
            assert!(parser.is_ground());
            assert_eq!(recorder.0.last(), Some(&Event::Print('Z')));
        }
    }
}

#[test]
fn overflowing_osc_does_not_poison_following_sequences() {
    let mut parser = Parser::<8>::new_with_size();
    let mut recorder = Recorder::default();
    parser.advance(b"\x1b]2;", &mut recorder);
    let count = if cfg!(miri) { 128 } else { 10_000 };
    for _ in 0..count {
        parser.advance(b"1234567890;", &mut recorder);
    }
    parser.advance(b"\x07\x1b]0;ok\x07\x1b[31mX", &mut recorder);
    assert_eq!(
        recorder.0,
        vec![
            Event::Osc(vec![b"2".to_vec(), b"1234567".to_vec()], true),
            Event::Osc(vec![b"0".to_vec(), b"ok".to_vec()], true),
            csi(&[&[31]], b"", false, 'm'),
            Event::Print('X'),
        ]
    );
}

#[test]
fn parser_is_send_and_sync() {
    fn check<T: Send + Sync>() {}
    check::<Parser>();
    check::<Parser<0>>();
}

#[test]
fn osc_overflow_is_reported_once_before_dispatch_per_command() {
    #[derive(Default)]
    struct Handler(Vec<&'static str>);
    impl Perform for Handler {
        fn osc_overflow(&mut self) {
            self.0.push("overflow");
        }
        fn osc_dispatch(&mut self, _: &[&[u8]], _: bool) {
            self.0.push("dispatch");
        }
    }
    let mut handler = Handler::default();
    let mut parser = Parser::<4>::new_with_size();
    parser.advance(b"\x1b]abcd\x07", &mut handler);
    assert_eq!(handler.0, ["dispatch"]);
    parser.advance(b"\x1b]abcde", &mut handler);
    assert_eq!(handler.0, ["dispatch", "overflow"]);
    parser.advance(b"fghijk;;;;\x07\x1b]12345\x07", &mut handler);
    assert_eq!(
        handler.0,
        ["dispatch", "overflow", "dispatch", "overflow", "dispatch"]
    );

    handler.0.clear();
    parser.advance(b"\x1b];;;;;;;;;;;;;;;\x07", &mut handler);
    assert_eq!(handler.0, ["dispatch"]);
    parser.advance(b"\x1b];;;;;;;;;;;;;;;;\x07", &mut handler);
    assert_eq!(handler.0, ["dispatch", "overflow", "dispatch"]);
}

#[test]
fn parameter_equality_ignores_previous_command_storage() {
    #[derive(Default)]
    struct Handler(Vec<Params>);
    impl Perform for Handler {
        fn csi_dispatch(&mut self, params: &Params, _: &[u8], _: bool, _: char) {
            self.0.push(params.clone());
        }
    }
    let mut reused = Handler::default();
    Parser::new().advance(b"\x1b[1:2;3;4m\x1b[7m", &mut reused);
    let mut fresh = Handler::default();
    Parser::new().advance(b"\x1b[7m", &mut fresh);
    assert_eq!(reused.0[1], fresh.0[0]);
    assert_ne!(reused.0[0], fresh.0[0]);
}
