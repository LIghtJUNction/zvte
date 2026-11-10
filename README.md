# zvte

[![CI](https://github.com/LIghtJUNction/zvte/actions/workflows/ci.yml/badge.svg)](https://github.com/LIghtJUNction/zvte/actions/workflows/ci.yml)

An incremental ANSI/VT parser for terminal output. The published crate always uses
`#![no_std]`, only `core`, zero dependencies and zero heap allocations. All mutable
state and bounded buffers live in `Parser`. Stable Rust **1.74.0** or newer.

```toml
[dependencies]
zvte = "0.1"
```

## Use

```rust
use zvte::{Params, Parser, Perform};

#[derive(Default)]
struct Output { characters: usize, controls: usize, sgr: usize }

impl Perform for Output {
    fn print(&mut self, _: char) { self.characters += 1; }
    fn execute(&mut self, _: u8) { self.controls += 1; }
    fn csi_dispatch(&mut self, _: &Params, intermediates: &[u8], ignore: bool, action: char) {
        if !ignore && intermediates.is_empty() && action == 'm' { self.sgr += 1; }
    }
}

let mut parser = Parser::new();
let mut output = Output::default();
parser.advance(b"\x1b[31", &mut output);
parser.advance(b"mred\x1b[0m\n", &mut output);
assert_eq!((output.characters, output.controls, output.sgr), (3, 1, 2));
assert!(parser.is_ground());
```

Chunks may split any control sequence or UTF-8 codepoint. Empty chunks do nothing.
Call `finish(&mut handler)` only at the end of the byte stream: it emits one
replacement character for unfinished UTF-8, unhooks active DCS data, discards
incomplete control commands and resets. `reset()` abandons a stream without callbacks.

To inspect a real stream:

```sh
printf '\033[31mred\033[0m\n' | cargo run --example inspect
```

## Callbacks

| Method | Event |
| --- | --- |
| `print(char)` | One printable Unicode scalar |
| `text(&str)` | A contiguous printable ASCII run; defaults to calling `print` for each character |
| `execute(u8)` | A C0 or C1 control |
| `esc_dispatch` | ESC final byte and intermediates |
| `csi_dispatch` | CSI parameters, subparameters, intermediates and final byte |
| `hook`, `put`, `unhook` | DCS header, payload bytes and end |
| `osc_dispatch` | OSC fields and whether BEL terminated the command |
| `osc_overflow` | First OSC capacity overflow in the current command |

Override `text` when your consumer can process a slice efficiently. It contains
printable ASCII, including DEL for `vte` compatibility; Unicode scalars use `print`.
Text callback boundaries follow input chunk boundaries. Preserve event order when
buffering output. The parser recognizes syntax; rendering, SGR interpretation,
PTYs, screen storage and terminal applications belong to the caller.

## Bounds and compatibility

The reference is [`alacritty/vte` **0.15.0**](https://github.com/alacritty/vte/tree/v0.15.0),
with its `std` feature disabled and the same OSC capacity. The comparison project
pins the registry version and lockfile. Semantics include:

- CSI/DCS have up to **32 numeric entries**, including colon subparameters. Empty
  entries become zero. Decimal values saturate at `65535`; extra entries set
  `ignore`. At most **two** intermediate/private marker bytes are retained.
- OSC stores **1024 payload bytes** and **16 fields** by default. Separators do not
  consume payload space. `Parser::<4096>::new_with_size()` changes storage at compile
  time; `Parser::<0>` is supported. Excess payload/fields are bounded and observable
  through `osc_overflow()`. Subsequent sequences continue normally.
- BEL, ESC, CAN and SUB terminate OSC. ESC completes OSC immediately; a following
  backslash dispatches a separate ESC event. Raw `0x9c` remains OSC data, matching
  `vte`'s UTF-8 semantics. DCS does recognize raw `0x9c` as its terminator.
- The seven-bit ESC forms introduce sequences. Raw C1 bytes in ground are
  reported through `execute`; they do not start eight-bit CSI/OSC commands.
- SOS/PM/APC are consumed until cancellation/ESC. Invalid parameter ordering is
  discarded and parsing resynchronizes. Invalid UTF-8 uses replacement characters
  without losing following controls. Truncation waits for the next chunk.

**One intentional correction:** `vte 0.15.0` reports UTF-8-encoded C1 as `print`
when its two bytes span input calls, but as `execute` in a single call. `zvte`
consistently uses `execute`. A dedicated regression test records both behaviors;
random differential tests allow only this named oracle defect. Timed fixtures
require exact callback/event equality without normalization. `text`,
`osc_overflow`, `finish`, and `reset` are additional APIs.

The core forbids unsafe code. Validated word-at-a-time ASCII scanning uses ordinary
unaligned-safe byte copies and works without target CPU feature flags. The constant
VT transition table avoids a large nested branch tree. The parser owns no external
resource, performs no I/O, and can be moved between threads. Handler panics remain
the caller's responsibility.

## Verify and measure

```sh
cargo test --locked
cargo +1.74.0 test --locked
cargo clippy --all-targets -- -D warnings
cargo doc --no-deps
cargo +nightly miri test
cargo test --manifest-path comparison/Cargo.toml --locked
cargo run --release --manifest-path comparison/Cargo.toml --bin bench
```

[`comparison/`](comparison/) is a separate Cargo workspace and is excluded from
the published package and dependency tree. Its fixtures, differential tests,
benchmark protocol and result files make performance claims reproducible. Strict
per-character comparisons and batched-text measurements are reported separately.
An allocator test verifies construction, parsing, finishing and resetting allocate
nothing. CI additionally tests Linux x86_64/aarch64, macOS and Windows, runs Miri,
and compiles WASM, Cortex-M and RISC-V targets at the MSRV.

## Release

In [Actions → Release](https://github.com/LIghtJUNction/zvte/actions/workflows/release.yml),
run the workflow on `main` and select `patch`, `minor`, `major`, or `none`.
`none` publishes the current version or resumes a partially completed publication.
Store the crates.io token in the repository secret **`CARGO_REGISTRY_TOKEN`**.

The workflow runs the complete CI gate, validates the selected version and package,
runs `cargo publish --dry-run`, pushes a version commit when needed, publishes to
crates.io, creates the matching version tag and creates a GitHub Release with the
`.crate` archive. All steps run in the same trusted manual workflow. PRs cannot
publish, and releases do not depend on a bot-created tag triggering another workflow.

If publication stops after the version commit is pushed, run a new workflow from
the current `main` with `none`. Already-published versions are accepted only when
their archive checksum matches; a conflicting version or tag fails closed.

MIT licensed. The implementation is independent; `vte` is used as a test oracle.
