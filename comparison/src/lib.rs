//! Event-level compatibility checks and deterministic benchmark workloads.
use std::fmt;

/// Every callback is kept, including control bytes and string terminators.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Print(char),
    Execute(u8),
    Hook(Vec<Vec<u16>>, Vec<u8>, bool, char),
    Put(u8),
    Unhook,
    Osc(Vec<Vec<u8>>, bool),
    Csi(Vec<Vec<u16>>, Vec<u8>, bool, char),
    Esc(Vec<u8>, bool, u8),
}

#[derive(Default, Debug)]
pub struct Events(pub Vec<Event>);

macro_rules! common_callbacks {
    () => {
        fn print(&mut self, c: char) {
            self.0.push(Event::Print(c));
        }
        fn execute(&mut self, byte: u8) {
            self.0.push(Event::Execute(byte));
        }
        fn put(&mut self, byte: u8) {
            self.0.push(Event::Put(byte));
        }
        fn unhook(&mut self) {
            self.0.push(Event::Unhook);
        }
        fn osc_dispatch(&mut self, params: &[&[u8]], bell: bool) {
            self.0.push(Event::Osc(
                params.iter().map(|s| s.to_vec()).collect(),
                bell,
            ));
        }
        fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
            self.0
                .push(Event::Esc(intermediates.to_vec(), ignore, byte));
        }
    };
}
impl zvte::Perform for Events {
    common_callbacks!();
    fn csi_dispatch(
        &mut self,
        params: &zvte::Params,
        intermediates: &[u8],
        ignore: bool,
        action: char,
    ) {
        self.0.push(Event::Csi(
            params.iter().map(|p| p.to_vec()).collect(),
            intermediates.to_vec(),
            ignore,
            action,
        ));
    }
    fn hook(&mut self, params: &zvte::Params, intermediates: &[u8], ignore: bool, action: char) {
        self.0.push(Event::Hook(
            params.iter().map(|p| p.to_vec()).collect(),
            intermediates.to_vec(),
            ignore,
            action,
        ));
    }
}
impl vte::Perform for Events {
    common_callbacks!();
    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        ignore: bool,
        action: char,
    ) {
        self.0.push(Event::Csi(
            params.iter().map(|p| p.to_vec()).collect(),
            intermediates.to_vec(),
            ignore,
            action,
        ));
    }
    fn hook(&mut self, params: &vte::Params, intermediates: &[u8], ignore: bool, action: char) {
        self.0.push(Event::Hook(
            params.iter().map(|p| p.to_vec()).collect(),
            intermediates.to_vec(),
            ignore,
            action,
        ));
    }
}

/// Feed the exact same boundaries to both parsers. No event normalization.
pub fn event_streams<const N: usize>(chunks: &[&[u8]]) -> (Vec<Event>, Vec<Event>) {
    let mut z = zvte::Parser::<N>::new_with_size();
    let mut v = vte::Parser::<N>::new_with_size();
    let mut ze = Events::default();
    let mut ve = Events::default();
    for chunk in chunks {
        z.advance(chunk, &mut ze);
        v.advance(&mut ve, chunk);
    }
    (ze.0, ve.0)
}

pub fn verify(data: &[u8], chunk: usize) -> Result<(), String> {
    let chunks: Vec<_> = data.chunks(chunk.max(1)).collect();
    let (actual, expected) = event_streams::<1024>(&chunks);
    if actual != expected {
        let index = actual
            .iter()
            .zip(&expected)
            .position(|(a, b)| a != b)
            .unwrap_or(actual.len().min(expected.len()));
        return Err(format!(
            "event {index}: zvte={:?}, vte={:?}; counts {} vs {} ({} bytes, chunk {chunk})",
            actual.get(index),
            expected.get(index),
            actual.len(),
            expected.len(),
            data.len()
        ));
    }
    Ok(())
}

pub struct Workload {
    pub name: &'static str,
    pub data: Vec<u8>,
    pub chunk: usize,
}
impl fmt::Debug for Workload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Workload")
            .field("name", &self.name)
            .field("bytes", &self.data.len())
            .field("chunk", &self.chunk)
            .finish()
    }
}
fn repeated(bytes: &[u8], n: usize) -> Vec<u8> {
    bytes.repeat(n)
}

/// Includes a checked-in recording made with a real pseudoterminal.
pub fn workloads() -> Vec<Workload> {
    let ascii = repeated(
        b"The quick brown fox jumps over the lazy dog. 0123456789 / terminal output\r\n",
        1024,
    );
    let csi = repeated(
        b"\x1b[38;2;255;128;0m\x1b[1;32m\x1b[4:3m\x1b[12;40H\x1b[?25l\x1b[0m",
        1024,
    );
    let osc = repeated(
        b"\x1b]0;build running\x07\x1b]8;id=1;https://example.com\x1b\\link\x1b]8;;\x1b\\",
        1024,
    );
    let logs = repeated(b"\x1b[32m INFO\x1b[0m worker=17 request completed status=200 bytes=4096 latency=0.24ms\r\n\x1b[33m WARN\x1b[0m retry scheduled attempt=2\r\n", 768);
    let dcs = repeated(
        b"\x1bP1;2$qpayload0123456789\x1b\\\x1bPq~?ABCDEFGHIJKLM\x1b\\",
        1024,
    );
    let unicode = repeated(
        "\x1b[36m终端输出：你好 🦀 café Ελληνικά русский\x1b[0m\r\n".as_bytes(),
        1024,
    );
    let unicode_cjk = repeated("终端输出状态更新数据处理网络连接测试完成".as_bytes(), 1024);
    let unicode_two_byte = repeated("Ελληνικάрусскийالعربيةעבריתéøñö".as_bytes(), 1024);
    let unicode_emoji = repeated("🦀🚀🌍🔒🧪💻🔥📦🌈🛰".as_bytes(), 1536);
    let mut unicode_mixed = String::new();
    let messages = [
        "连接成功：终端已就绪 🦀",
        "Ελληνικά русский café",
        "مرحبا بالعالم 🌍",
        "日本語の出力テスト 🧪",
    ];
    for i in 0..1024 {
        unicode_mixed.push_str(&format!(
            "request={i}\t\x1b[38;2;12;34;56m{}\x1b[0m bytes={}\r\n",
            messages[i % messages.len()],
            i * 17,
        ));
    }
    let unicode_mixed = unicode_mixed.into_bytes();
    let malformed = repeated(b"ok\x80\xff\xc0\xaf\xed\xa0\x80\xe2x\x1b[1?2m\x1b[:::::::::::::::::::::::::::::::::m\x1bP?1<qdiscard\x18\x1b]0;cancel\x1a", 512);
    let mut truncated = repeated(b"\x1b[31mhello\x1b[0m\r\n", 2048);
    truncated.extend_from_slice(b"\x1b[12;9");
    vec![
        Workload {
            name: "ascii",
            chunk: ascii.len(),
            data: ascii,
        },
        Workload {
            name: "csi",
            chunk: csi.len(),
            data: csi,
        },
        Workload {
            name: "osc",
            chunk: osc.len(),
            data: osc,
        },
        Workload {
            name: "color-log",
            chunk: logs.len(),
            data: logs.clone(),
        },
        Workload {
            name: "short-chunks",
            chunk: 16,
            data: logs.clone(),
        },
        Workload {
            name: "one-byte",
            chunk: 1,
            data: logs,
        },
        Workload {
            name: "dcs",
            chunk: dcs.len(),
            data: dcs,
        },
        Workload {
            name: "unicode",
            chunk: unicode.len(),
            data: unicode,
        },
        Workload {
            name: "unicode-cjk",
            chunk: unicode_cjk.len(),
            data: unicode_cjk.clone(),
        },
        Workload {
            name: "unicode-two-byte",
            chunk: unicode_two_byte.len(),
            data: unicode_two_byte,
        },
        Workload {
            name: "unicode-emoji",
            chunk: unicode_emoji.len(),
            data: unicode_emoji.clone(),
        },
        Workload {
            name: "unicode-mixed",
            chunk: unicode_mixed.len(),
            data: unicode_mixed,
        },
        Workload {
            name: "unicode-short-chunks",
            chunk: 17,
            data: unicode_cjk,
        },
        Workload {
            name: "unicode-one-byte",
            chunk: 1,
            data: unicode_emoji,
        },
        Workload {
            name: "recording",
            chunk: 4096,
            data: include_bytes!("../fixtures/terminal-session.ansi").to_vec(),
        },
        Workload {
            name: "malformed",
            chunk: 31,
            data: malformed,
        },
        Workload {
            name: "truncated",
            chunk: 4096,
            data: truncated,
        },
    ]
}
