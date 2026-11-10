//! Zero-dependency, allocation-free ANSI/VT terminal parsing.
//!
//! Feed arbitrary byte chunks to [`Parser::advance`]. Implement [`Perform`] to
//! consume decoded text and control sequences; override [`Perform::text`] to
//! receive contiguous printable ASCII in batches. The default implementation
//! preserves one [`Perform::print`] callback per character.
//!
//! ```
//! use zvte::{Parser, Perform, Params};
//!
//! struct Screen;
//! impl Perform for Screen {
//!     fn print(&mut self, c: char) { /* draw c */ let _ = c; }
//!     fn csi_dispatch(&mut self, p: &Params, i: &[u8], ignore: bool, action: char) {
//!         if !ignore && i.is_empty() && action == 'm' {
//!             for parameter in p { let _ = parameter; /* apply SGR */ }
//!         }
//!     }
//! }
//! let mut parser = Parser::new();
//! parser.advance(b"\x1b[31mred\x1b[0m", &mut Screen);
//! ```
//!
//! Control sequence semantics follow `vte 0.15.0` in its bounded, no-std mode:
//! 32 numeric entries, two intermediate bytes, 16 OSC fields, UTF-8 text and
//! seven-bit sequence introducers. Raw C1 bytes in text are executed. See the
//! README for EOF behavior, capacity limits and the split UTF-8 C1 correction.
#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod params;
mod scan;
mod state;

pub use params::{Params, ParamsIter, MAX_PARAMS};
use state::*;

/// Default number of stored OSC payload bytes (separators consume no space).
pub const DEFAULT_OSC_CAPACITY: usize = 1024;
/// Maximum number of semicolon-separated OSC fields passed to the handler.
pub const MAX_OSC_PARAMS: usize = 16;
/// Maximum number of stored private-marker and intermediate bytes.
pub const MAX_INTERMEDIATES: usize = 2;

/// Incremental ANSI/VT parser with all state and storage held inline.
///
/// `N` is the OSC payload capacity. Use [`Parser::new_with_size`] to customize
/// it, including zero on memory-constrained targets. Exceeding parameter or
/// intermediate capacity sets the dispatch `ignore` flag; overflowing OSC
/// data is truncated, with [`Perform::osc_overflow`] called once per command.
///
/// A parser can be moved between threads; one byte stream requires one parser.
/// Handler panics propagate normally. Parser operations themselves do not panic
/// for any byte input. No allocation, global mutable state or unsafe code is used.
#[derive(Clone)]
pub struct Parser<const N: usize = DEFAULT_OSC_CAPACITY> {
    osc: [u8; N],
    osc_ends: [usize; MAX_OSC_PARAMS],
    osc_len: usize,
    params: Params,
    param: u16,
    utf8: [u8; 4],
    intermediates: [u8; MAX_INTERMEDIATES],
    state: u8,
    intermediate_len: u8,
    osc_count: u8,
    utf8_len: u8,
    ignoring: bool,
    osc_overflow: bool,
}

impl Parser {
    /// Create a parser with a 1024-byte inline OSC buffer.
    pub const fn new() -> Self {
        Self::new_with_size()
    }
}

impl<const N: usize> Default for Parser<N> {
    fn default() -> Self {
        Self::new_with_size()
    }
}

impl<const N: usize> Parser<N> {
    /// Create a parser with an explicit inline OSC payload capacity.
    pub const fn new_with_size() -> Self {
        Self {
            osc: [0; N],
            osc_ends: [0; MAX_OSC_PARAMS],
            osc_len: 0,
            params: Params::new(),
            param: 0,
            utf8: [0; 4],
            intermediates: [0; MAX_INTERMEDIATES],
            state: GROUND,
            intermediate_len: 0,
            osc_count: 0,
            utf8_len: 0,
            ignoring: false,
            osc_overflow: false,
        }
    }

    /// Whether no control sequence or partial UTF-8 codepoint is pending.
    #[inline]
    pub const fn is_ground(&self) -> bool {
        self.state == GROUND && self.utf8_len == 0
    }

    /// Discard all pending input without issuing callbacks or clearing storage.
    pub fn reset(&mut self) {
        self.state = GROUND;
        self.utf8_len = 0;
        self.osc_len = 0;
        self.osc_count = 0;
        self.osc_overflow = false;
        self.clear_params();
    }

    /// Finish a stream: replace incomplete UTF-8, unhook an active DCS, then reset.
    ///
    /// Incomplete ESC/CSI/OSC commands are discarded. Ordinary chunk boundaries
    /// must use [`Self::advance`] without calling this method. Repeated calls
    /// without intervening input have no effect.
    pub fn finish<P: Perform + ?Sized>(&mut self, handler: &mut P) {
        if self.utf8_len != 0 {
            handler.print('\u{fffd}');
        }
        if self.state == DCS_DATA {
            handler.unhook();
        }
        self.reset();
    }

    /// Parse a chunk, preserving incomplete sequences for the next call.
    ///
    /// Empty input is a no-op. Callbacks borrow parser/input storage only for
    /// the duration of the call. Batch boundaries depend on the input chunks;
    /// the expanded character/control event stream does not.
    #[inline]
    pub fn advance<P: Perform + ?Sized>(&mut self, input: &[u8], handler: &mut P) {
        let mut i = 0;
        while i < input.len() {
            if self.state == GROUND {
                if self.utf8_len != 0 {
                    i += self.complete_utf8(&input[i..], handler);
                    continue;
                }
                let run = scan::ascii_run(&input[i..]);
                if run != 0 {
                    handler.text(&input[i..i + run]);
                    i += run;
                    continue;
                }
                let byte = input[i];
                if byte == 0x1b {
                    self.clear_params();
                    self.state = ESCAPE;
                    i += 1;
                    match input.get(i) {
                        Some(b'[') => {
                            self.state = CSI_ENTRY;
                            i += 1;
                        }
                        Some(b']') => {
                            self.state = OSC;
                            self.osc_len = 0;
                            self.osc_count = 0;
                            self.osc_overflow = false;
                            i += 1;
                        }
                        Some(b'P') => {
                            self.state = DCS_ENTRY;
                            i += 1;
                        }
                        _ => {}
                    }
                } else if byte < 0x20 || (0x80..=0x9f).contains(&byte) {
                    handler.execute(byte);
                    i += 1;
                } else {
                    i += self.start_utf8(&input[i..], handler);
                }
            } else if self.state == OSC {
                let run = scan::osc_run(&input[i..]);
                if run != 0 {
                    let stored = run.min(N - self.osc_len);
                    self.osc[self.osc_len..self.osc_len + stored]
                        .copy_from_slice(&input[i..i + stored]);
                    self.osc_len += stored;
                    if stored != run {
                        self.report_osc_overflow(handler);
                    }
                    i += run;
                } else {
                    self.step(input[i], handler);
                    i += 1;
                }
            } else if matches!(self.state, CSI_ENTRY | CSI_PARAM | DCS_ENTRY | DCS_PARAM) {
                i += self.header(&input[i..], handler);
            } else if self.state == DCS_DATA {
                let start = i;
                while i < input.len() && matches!(input[i], 0x00..=0x17 | 0x19 | 0x1c..=0x7e) {
                    i += 1;
                }
                for &byte in &input[start..i] {
                    handler.put(byte);
                }
                if i < input.len() {
                    self.step(input[i], handler);
                    i += 1;
                }
            } else {
                let byte = input[i];
                self.step(byte, handler);
                i += 1;
            }
        }
    }

    #[inline]
    fn header<P: Perform + ?Sized>(&mut self, input: &[u8], handler: &mut P) -> usize {
        let param_state = if self.state >= DCS_ENTRY {
            DCS_PARAM
        } else {
            CSI_PARAM
        };
        let mut i = 0;
        while i < input.len() {
            let byte = input[i];
            let digit = byte.wrapping_sub(b'0');
            if digit < 10 {
                self.state = param_state;
                if !self.params.full() {
                    let mut value = self.param as u32;
                    while i < input.len() && input[i].is_ascii_digit() {
                        value = (value * 10 + (input[i] - b'0') as u32).min(u16::MAX as u32);
                        i += 1;
                    }
                    self.param = value as u16;
                } else {
                    self.ignoring = true;
                    i += 1;
                }
            } else if byte == b';' || byte == b':' {
                self.state = param_state;
                if self.params.push(self.param, byte == b':') {
                    self.param = 0;
                } else {
                    self.ignoring = true;
                }
                i += 1;
            } else {
                if (0x40..=0x7e).contains(&byte) {
                    if !self.params.push(self.param, false) {
                        self.ignoring = true;
                    }
                    let intermediates = &self.intermediates[..self.intermediate_len as usize];
                    if param_state == DCS_PARAM {
                        self.state = DCS_DATA;
                        handler.hook(&self.params, intermediates, self.ignoring, byte as char);
                    } else {
                        self.state = GROUND;
                        handler.csi_dispatch(
                            &self.params,
                            intermediates,
                            self.ignoring,
                            byte as char,
                        );
                    }
                    return i + 1;
                }
                self.step(byte, handler);
                i += 1;
                if self.state != param_state {
                    break;
                }
            }
        }
        i
    }

    #[inline]
    fn clear_params(&mut self) {
        self.params.clear();
        self.param = 0;
        self.intermediate_len = 0;
        self.ignoring = false;
    }

    #[inline]
    fn step<P: Perform + ?Sized>(&mut self, byte: u8, handler: &mut P) {
        let transition = TRANSITIONS[self.state as usize][byte as usize];
        self.state = (transition & 15) as u8;
        match (transition >> 4) as u8 {
            IGNORE => {}
            EXECUTE => handler.execute(byte),
            COLLECT => {
                if (self.intermediate_len as usize) < MAX_INTERMEDIATES {
                    self.intermediates[self.intermediate_len as usize] = byte;
                    self.intermediate_len += 1;
                } else {
                    self.ignoring = true;
                }
            }
            DIGIT => {
                if self.params.full() {
                    self.ignoring = true;
                } else {
                    self.param =
                        (self.param as u32 * 10 + (byte - b'0') as u32).min(u16::MAX as u32) as u16;
                }
            }
            PARAM | SUBPARAM => {
                if self
                    .params
                    .push(self.param, (transition >> 4) as u8 == SUBPARAM)
                {
                    self.param = 0;
                } else {
                    self.ignoring = true;
                }
            }
            ESC_DISPATCH => handler.esc_dispatch(
                &self.intermediates[..self.intermediate_len as usize],
                self.ignoring,
                byte,
            ),
            CSI_DISPATCH | HOOK => {
                if !self.params.push(self.param, false) {
                    self.ignoring = true;
                }
                let intermediates = &self.intermediates[..self.intermediate_len as usize];
                if (transition >> 4) as u8 == HOOK {
                    handler.hook(&self.params, intermediates, self.ignoring, byte as char);
                } else {
                    handler.csi_dispatch(&self.params, intermediates, self.ignoring, byte as char);
                }
            }
            PUT => handler.put(byte),
            UNHOOK => handler.unhook(),
            UNHOOK_EXECUTE => {
                handler.unhook();
                handler.execute(byte);
            }
            UNHOOK_CLEAR => {
                handler.unhook();
                self.clear_params();
            }
            OSC_END => self.osc_end(byte, handler),
            OSC_EXECUTE => {
                self.osc_end(byte, handler);
                handler.execute(byte);
            }
            OSC_CLEAR => {
                self.osc_end(byte, handler);
                self.clear_params();
            }
            OSC_PUT => {
                if self.osc_len < N {
                    self.osc[self.osc_len] = byte;
                    self.osc_len += 1;
                } else {
                    self.report_osc_overflow(handler);
                }
            }
            OSC_PARAM => {
                if self.osc_len < N {
                    if !self.osc_param() {
                        self.report_osc_overflow(handler);
                    }
                } else {
                    self.report_osc_overflow(handler);
                }
            }
            CLEAR => self.clear_params(),
            OSC_START => {
                self.osc_len = 0;
                self.osc_count = 0;
                self.osc_overflow = false;
            }
            _ => {}
        }
    }

    #[inline]
    fn report_osc_overflow<P: Perform + ?Sized>(&mut self, handler: &mut P) {
        if !self.osc_overflow {
            self.osc_overflow = true;
            handler.osc_overflow();
        }
    }

    #[inline]
    fn osc_param(&mut self) -> bool {
        if self.osc_count as usize == MAX_OSC_PARAMS {
            return false;
        }
        self.osc_ends[self.osc_count as usize] = self.osc_len;
        self.osc_count += 1;
        true
    }

    fn osc_end<P: Perform + ?Sized>(&mut self, byte: u8, handler: &mut P) {
        // Like vte, finalize the last field even when the payload buffer is full.
        if !self.osc_param() {
            self.report_osc_overflow(handler);
        }
        let mut fields: [&[u8]; MAX_OSC_PARAMS] = [&[]; MAX_OSC_PARAMS];
        let mut start = 0;
        for (field, &end) in fields
            .iter_mut()
            .zip(&self.osc_ends[..self.osc_count as usize])
        {
            *field = &self.osc[start..end];
            start = end;
        }
        handler.osc_dispatch(&fields[..self.osc_count as usize], byte == 7);
        self.osc_len = 0;
        self.osc_count = 0;
    }

    #[inline]
    fn decoded<P: Perform + ?Sized>(handler: &mut P, c: char) {
        if c <= '\u{1f}' || ('\u{80}'..='\u{9f}').contains(&c) {
            handler.execute(c as u8);
        } else {
            handler.print(c);
        }
    }

    fn start_utf8<P: Perform + ?Sized>(&mut self, input: &[u8], handler: &mut P) -> usize {
        let len = scan::escape(input);
        match core::str::from_utf8(&input[..len]) {
            Ok(text) => {
                for c in text.chars() {
                    Self::decoded(handler, c);
                }
                len
            }
            Err(error) => {
                let valid = error.valid_up_to();
                if let Ok(text) = core::str::from_utf8(&input[..valid]) {
                    for c in text.chars() {
                        Self::decoded(handler, c);
                    }
                }
                match error.error_len() {
                    Some(invalid) => {
                        if invalid == 1 && input[valid] <= 0x9f {
                            handler.execute(input[valid]);
                        } else {
                            handler.print('\u{fffd}');
                        }
                        valid + invalid
                    }
                    None if len < input.len() => {
                        handler.print('\u{fffd}');
                        len
                    }
                    None => {
                        let partial = len - valid;
                        self.utf8[..partial].copy_from_slice(&input[valid..len]);
                        self.utf8_len = partial as u8;
                        len
                    }
                }
            }
        }
    }

    fn complete_utf8<P: Perform + ?Sized>(&mut self, input: &[u8], handler: &mut P) -> usize {
        let old = self.utf8_len as usize;
        let width = match self.utf8[0] {
            0xc2..=0xdf => 2,
            0xe0..=0xef => 3,
            _ => 4,
        };
        let take = input.len().min(width - old);
        self.utf8[old..old + take].copy_from_slice(&input[..take]);
        match core::str::from_utf8(&self.utf8[..old + take]) {
            Ok(text) => {
                if let Some(c) = text.chars().next() {
                    Self::decoded(handler, c);
                }
                self.utf8_len = 0;
                take
            }
            Err(error) => match error.error_len() {
                Some(invalid) => {
                    self.utf8_len = 0;
                    handler.print('\u{fffd}');
                    invalid.saturating_sub(old)
                }
                None => {
                    self.utf8_len = (old + take) as u8;
                    take
                }
            },
        }
    }
}

/// Consumer of terminal parsing events. Unimplemented methods discard events.
///
/// With the exception of `text` and `osc_overflow`, callback names and arguments
/// mirror `vte::Perform`. No allocation is required to implement this trait.
pub trait Perform {
    /// One decoded printable Unicode scalar, including DEL.
    fn print(&mut self, _c: char) {}
    /// A run of printable ASCII bytes. Defaults to one [`Self::print`] per byte.
    ///
    /// Overriding this method reduces callback overhead; preserve order relative
    /// to all other callbacks. Call boundaries are not part of text semantics.
    #[inline]
    /// Every byte is in `0x20..=0x7f`; no UTF-8 validation is needed to consume
    /// the bytes. The byte-slice API avoids redundant validation or unsafe casts.
    fn text(&mut self, text: &[u8]) {
        for &byte in text {
            self.print(byte as char);
        }
    }
    /// Execute a C0 or C1 control byte.
    fn execute(&mut self, _byte: u8) {}
    /// Complete ESC command. `ignore` signals intermediate overflow.
    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}
    /// Complete CSI command. `ignore` signals parameter/intermediate overflow.
    fn csi_dispatch(
        &mut self,
        _params: &Params,
        _intermediates: &[u8],
        _ignore: bool,
        _action: char,
    ) {
    }
    /// Start DCS data after its final header byte, with parsed parameters.
    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {}
    /// One DCS data byte; DEL and high bytes other than ST are ignored.
    fn put(&mut self, _byte: u8) {}
    /// End or cancel the active DCS data stream.
    fn unhook(&mut self) {}
    /// Complete OSC command. `bell_terminated` is true only for BEL termination.
    ///
    /// ESC ends the OSC immediately; a following `\\` is a separate ESC event.
    /// CAN/SUB also dispatch OSC before their execute callback, matching vte.
    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}
    /// OSC storage or field capacity was exceeded, once per OSC command.
    ///
    /// The eventual dispatch contains the bounded prefix. This extra callback
    /// makes truncation observable without changing vte-compatible dispatches.
    fn osc_overflow(&mut self) {}
}
