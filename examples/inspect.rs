//! Run with `printf '\033[31mred\033[0m\n' | cargo run --example inspect`.
use std::io::{self, Read};
use zvte::{Params, Parser, Perform};

struct Inspector;

impl Perform for Inspector {
    fn text(&mut self, text: &[u8]) {
        println!("text {:?}", String::from_utf8_lossy(text));
    }

    fn print(&mut self, c: char) {
        println!("character {c:?}");
    }

    fn execute(&mut self, byte: u8) {
        println!("control {byte:#04x}");
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
        println!("CSI {params:?} {intermediates:?} {action:?}, ignore={ignore}");
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        println!("OSC {params:?}, bell={bell_terminated}");
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        println!("ESC {intermediates:?} {byte:#04x}, ignore={ignore}");
    }

    fn hook(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
        println!("DCS begin {params:?} {intermediates:?} {action:?}, ignore={ignore}");
    }

    fn put(&mut self, byte: u8) {
        println!("DCS byte {byte:#04x}");
    }

    fn unhook(&mut self) {
        println!("DCS end");
    }
}

fn main() -> io::Result<()> {
    let mut parser = Parser::new();
    let mut inspector = Inspector;
    let mut input = io::stdin().lock();
    let mut buffer = [0; 4096];
    loop {
        let count = match input.read(&mut buffer) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if count == 0 {
            break;
        }
        parser.advance(&buffer[..count], &mut inspector);
    }
    parser.finish(&mut inspector);
    Ok(())
}
