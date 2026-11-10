//! Constant transition table for the non-text portion of the VT grammar.
//! Each entry packs a four-bit next state and a five-bit action.

pub(crate) const GROUND: u8 = 0;
pub(crate) const ESCAPE: u8 = 1;
pub(crate) const ESC_INTERMEDIATE: u8 = 2;
pub(crate) const CSI_ENTRY: u8 = 3;
pub(crate) const CSI_PARAM: u8 = 4;
pub(crate) const CSI_INTERMEDIATE: u8 = 5;
pub(crate) const CSI_IGNORE: u8 = 6;
pub(crate) const DCS_ENTRY: u8 = 7;
pub(crate) const DCS_PARAM: u8 = 8;
pub(crate) const DCS_INTERMEDIATE: u8 = 9;
pub(crate) const DCS_IGNORE: u8 = 10;
pub(crate) const DCS_DATA: u8 = 11;
pub(crate) const OSC: u8 = 12;
pub(crate) const STRING: u8 = 13;

pub(crate) const IGNORE: u8 = 0;
pub(crate) const EXECUTE: u8 = 1;
pub(crate) const COLLECT: u8 = 2;
pub(crate) const DIGIT: u8 = 3;
pub(crate) const PARAM: u8 = 4;
pub(crate) const SUBPARAM: u8 = 5;
pub(crate) const ESC_DISPATCH: u8 = 6;
pub(crate) const CSI_DISPATCH: u8 = 7;
pub(crate) const HOOK: u8 = 8;
pub(crate) const PUT: u8 = 9;
pub(crate) const UNHOOK: u8 = 10;
pub(crate) const UNHOOK_EXECUTE: u8 = 11;
pub(crate) const UNHOOK_CLEAR: u8 = 12;
pub(crate) const OSC_END: u8 = 13;
pub(crate) const OSC_EXECUTE: u8 = 14;
pub(crate) const OSC_CLEAR: u8 = 15;
pub(crate) const OSC_PUT: u8 = 16;
pub(crate) const OSC_PARAM: u8 = 17;
pub(crate) const CLEAR: u8 = 18;
pub(crate) const OSC_START: u8 = 19;

const fn entry(state: u8, action: u8) -> u16 {
    state as u16 | ((action as u16) << 4)
}

const fn transition(state: u8, byte: u8) -> u16 {
    if state == OSC {
        return match byte {
            0x07 => entry(GROUND, OSC_END),
            0x18 | 0x1a => entry(GROUND, OSC_EXECUTE),
            0x1b => entry(ESCAPE, OSC_CLEAR),
            0x00..=0x1f => entry(OSC, IGNORE),
            b';' => entry(OSC, OSC_PARAM),
            _ => entry(OSC, OSC_PUT),
        };
    }
    if state == DCS_DATA {
        return match byte {
            0x18 | 0x1a => entry(GROUND, UNHOOK_EXECUTE),
            0x1b => entry(ESCAPE, UNHOOK_CLEAR),
            0x9c => entry(GROUND, UNHOOK),
            0x00..=0x7e => entry(DCS_DATA, PUT),
            _ => entry(DCS_DATA, IGNORE),
        };
    }
    if byte == 0x18 || byte == 0x1a {
        return entry(GROUND, EXECUTE);
    }
    if byte == 0x1b {
        return entry(ESCAPE, if state == ESCAPE { IGNORE } else { CLEAR });
    }
    if state == DCS_IGNORE || state == STRING {
        return entry(state, IGNORE);
    }
    if byte < 0x20 {
        return entry(state, if state >= DCS_ENTRY { IGNORE } else { EXECUTE });
    }
    if state == ESCAPE {
        return match byte {
            0x20..=0x2f => entry(ESC_INTERMEDIATE, COLLECT),
            b'P' => entry(DCS_ENTRY, CLEAR),
            b'X' | b'^' | b'_' => entry(STRING, IGNORE),
            b'[' => entry(CSI_ENTRY, CLEAR),
            b']' => entry(OSC, OSC_START),
            0x30..=0x7e => entry(GROUND, ESC_DISPATCH),
            _ => entry(state, IGNORE),
        };
    }
    if state == ESC_INTERMEDIATE {
        return match byte {
            0x20..=0x2f => entry(state, COLLECT),
            0x30..=0x7e => entry(GROUND, ESC_DISPATCH),
            _ => entry(state, IGNORE),
        };
    }
    if state == CSI_IGNORE {
        return entry(
            if byte >= 0x40 && byte <= 0x7e {
                GROUND
            } else {
                state
            },
            IGNORE,
        );
    }
    let dcs = state >= DCS_ENTRY;
    let param_state = if dcs { DCS_PARAM } else { CSI_PARAM };
    let intermediate_state = if dcs {
        DCS_INTERMEDIATE
    } else {
        CSI_INTERMEDIATE
    };
    let ignore_state = if dcs { DCS_IGNORE } else { CSI_IGNORE };
    if byte >= 0x40 && byte <= 0x7e {
        return if dcs {
            entry(DCS_DATA, HOOK)
        } else {
            entry(GROUND, CSI_DISPATCH)
        };
    }
    if byte >= 0x20 && byte <= 0x2f {
        return entry(intermediate_state, COLLECT);
    }
    if byte >= 0x30 && byte <= 0x3f {
        if state == intermediate_state {
            return entry(ignore_state, IGNORE);
        }
        return match byte {
            b'0'..=b'9' => entry(param_state, DIGIT),
            b':' => entry(param_state, SUBPARAM),
            b';' => entry(param_state, PARAM),
            _ if state == param_state => entry(ignore_state, IGNORE),
            _ => entry(param_state, COLLECT),
        };
    }
    entry(state, IGNORE)
}

const fn table() -> [[u16; 256]; 14] {
    let mut result = [[0; 256]; 14];
    let mut state = 1;
    while state < 14 {
        let mut byte = 0;
        while byte < 256 {
            result[state][byte] = transition(state as u8, byte as u8);
            byte += 1;
        }
        state += 1;
    }
    result
}

pub(crate) static TRANSITIONS: [[u16; 256]; 14] = table();
