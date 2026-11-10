use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use zvte::{Params, Parser, Perform};

struct CountingAllocator;

thread_local! {
    static TRACK: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

fn record_allocation() {
    let _ = TRACK.try_with(|track| {
        if track.get() {
            let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        }
    });
}

// SAFETY: every allocation operation is forwarded unchanged to System. The
// thread-local counters contain no heap allocations and never touch the memory.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        // SAFETY: the caller supplies the Layout required by GlobalAlloc.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        // SAFETY: the caller supplies the Layout required by GlobalAlloc.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record_allocation();
        // SAFETY: the pointer and layout are forwarded without modification.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: all pointers come from the same System allocator.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[derive(Default)]
struct Handler(usize);
impl Perform for Handler {
    fn print(&mut self, c: char) {
        self.0 = self.0.wrapping_add(c as usize);
    }
    fn execute(&mut self, byte: u8) {
        self.0 = self.0.wrapping_add(byte as usize);
    }
    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
        self.0 = self
            .0
            .wrapping_add(params.len() + intermediates.len() + ignore as usize + action as usize);
    }
    fn osc_dispatch(&mut self, params: &[&[u8]], bell: bool) {
        for param in params {
            self.0 = self.0.wrapping_add(param.len());
        }
        self.0 = self.0.wrapping_add(bell as usize);
    }
    fn hook(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
        self.csi_dispatch(params, intermediates, ignore, action);
    }
    fn put(&mut self, byte: u8) {
        self.execute(byte);
    }
    fn unhook(&mut self) {
        self.0 = self.0.wrapping_add(1);
    }
    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        self.0 = self
            .0
            .wrapping_add(intermediates.len() + ignore as usize + byte as usize);
    }
}

#[test]
fn construction_parsing_eof_and_reset_do_not_allocate() {
    ALLOCATIONS.with(|count| count.set(0));
    TRACK.with(|track| track.set(true));
    let mut parser = Parser::new();
    let mut handler = Handler::default();
    let input = b"plain ASCII\n\xe4\xb8\xad\xf0\x9f\xa6\x80\x1b[38:2::1:2:3m\x1b]2;title\x07\x1bP1;2$qpayload\x1b\\\x1b(B\x1bXignored\x18\xe2\x82";
    for chunk in input.chunks(3) {
        parser.advance(chunk, &mut handler);
    }
    parser.finish(&mut handler);
    parser.reset();
    let mut tiny = Parser::<0>::new_with_size();
    tiny.advance(b"\x1b]huge;ignored\x07", &mut handler);
    TRACK.with(|track| track.set(false));
    let allocations = ALLOCATIONS.with(Cell::get);
    assert!(handler.0 > 0);
    assert_eq!(allocations, 0);
}
