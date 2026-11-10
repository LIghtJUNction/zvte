//! Reproducible wall-clock comparison; no third-party benchmark framework.
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use zvte_comparison::{verify, workloads, Workload};

struct CountingAllocator;
static COUNTING: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static ALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);
// Only the standalone benchmark instruments the standard allocator. The library
// under test has no allocator, unsafe code, globals, or instrumentation.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        }
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(size, Ordering::Relaxed);
        }
        unsafe { System.realloc(pointer, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// Identical arithmetic for both implementations; event payloads remain live.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Digest {
    value: u64,
    events: u64,
}
impl Digest {
    #[inline]
    fn event(&mut self, tag: u64, value: u64) {
        self.value = self
            .value
            .wrapping_add(value.wrapping_add(tag * 0x9e37_79b9));
        self.events += 1;
    }
    #[inline]
    fn bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.value = self.value.wrapping_add(u64::from(byte));
        }
    }
    #[inline]
    fn params<'a>(&mut self, params: impl Iterator<Item = &'a [u16]>) {
        for group in params {
            self.value = self.value.wrapping_add(0x10000 + group.len() as u64);
            for &value in group {
                self.value = self.value.wrapping_add(u64::from(value));
            }
        }
    }
}
macro_rules! scalar_callbacks {
    () => {
        #[inline]
        fn print(&mut self, c: char) {
            self.event(1, c as u64);
        }
        #[inline]
        fn execute(&mut self, byte: u8) {
            self.event(2, u64::from(byte));
        }
        #[inline]
        fn put(&mut self, byte: u8) {
            self.event(3, u64::from(byte));
        }
        #[inline]
        fn unhook(&mut self) {
            self.event(4, 0);
        }
        #[inline]
        fn osc_dispatch(&mut self, params: &[&[u8]], bell: bool) {
            self.event(5, u64::from(bell));
            for param in params {
                self.value = self.value.wrapping_add(0x10000 + param.len() as u64);
                self.bytes(param);
            }
        }
        #[inline]
        fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
            self.event(6, u64::from(byte) + u64::from(ignore));
            self.bytes(intermediates);
        }
    };
}
impl zvte::Perform for Digest {
    scalar_callbacks!();
    #[inline]
    fn csi_dispatch(
        &mut self,
        params: &zvte::Params,
        intermediates: &[u8],
        ignore: bool,
        action: char,
    ) {
        self.event(7, action as u64 + u64::from(ignore));
        self.params(params.iter());
        self.bytes(intermediates);
    }
    #[inline]
    fn hook(&mut self, params: &zvte::Params, intermediates: &[u8], ignore: bool, action: char) {
        self.event(8, action as u64 + u64::from(ignore));
        self.params(params.iter());
        self.bytes(intermediates);
    }
}
impl vte::Perform for Digest {
    scalar_callbacks!();
    #[inline]
    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        ignore: bool,
        action: char,
    ) {
        self.event(7, action as u64 + u64::from(ignore));
        self.params(params.iter());
        self.bytes(intermediates);
    }
    #[inline]
    fn hook(&mut self, params: &vte::Params, intermediates: &[u8], ignore: bool, action: char) {
        self.event(8, action as u64 + u64::from(ignore));
        self.params(params.iter());
        self.bytes(intermediates);
    }
}
#[derive(Default)]
struct Batch(Digest);
impl zvte::Perform for Batch {
    #[inline]
    fn text(&mut self, text: &[u8]) {
        let count = text.len() as u64;
        self.0.events += count;
        self.0.value = self.0.value.wrapping_add(count.wrapping_mul(0x9e37_79b9));
        self.0.bytes(text);
    }
    #[inline]
    fn print(&mut self, c: char) {
        zvte::Perform::print(&mut self.0, c);
    }
    #[inline]
    fn execute(&mut self, byte: u8) {
        zvte::Perform::execute(&mut self.0, byte);
    }
    #[inline]
    fn put(&mut self, byte: u8) {
        zvte::Perform::put(&mut self.0, byte);
    }
    #[inline]
    fn unhook(&mut self) {
        zvte::Perform::unhook(&mut self.0);
    }
    #[inline]
    fn osc_dispatch(&mut self, params: &[&[u8]], bell: bool) {
        zvte::Perform::osc_dispatch(&mut self.0, params, bell);
    }
    #[inline]
    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        zvte::Perform::esc_dispatch(&mut self.0, intermediates, ignore, byte);
    }
    #[inline]
    fn csi_dispatch(
        &mut self,
        params: &zvte::Params,
        intermediates: &[u8],
        ignore: bool,
        action: char,
    ) {
        zvte::Perform::csi_dispatch(&mut self.0, params, intermediates, ignore, action);
    }
    #[inline]
    fn hook(&mut self, params: &zvte::Params, intermediates: &[u8], ignore: bool, action: char) {
        zvte::Perform::hook(&mut self.0, params, intermediates, ignore, action);
    }
}
#[inline(never)]
fn zvte_once(workload: &Workload) -> Digest {
    let mut parser = zvte::Parser::new();
    let mut digest = Digest::default();
    for chunk in black_box(workload.data.as_slice()).chunks(workload.chunk) {
        parser.advance(black_box(chunk), &mut digest);
    }
    black_box(parser);
    black_box(digest)
}
#[inline(never)]
fn zvte_batch_once(workload: &Workload) -> Digest {
    let mut parser = zvte::Parser::new();
    let mut digest = Batch::default();
    for chunk in black_box(workload.data.as_slice()).chunks(workload.chunk) {
        parser.advance(black_box(chunk), &mut digest);
    }
    black_box(parser);
    black_box(digest.0)
}
#[inline(never)]
fn vte_once(workload: &Workload) -> Digest {
    let mut parser = vte::Parser::new();
    let mut digest = Digest::default();
    for chunk in black_box(workload.data.as_slice()).chunks(workload.chunk) {
        parser.advance(&mut digest, black_box(chunk));
    }
    black_box(parser);
    black_box(digest)
}
fn run(function: fn(&Workload) -> Digest, workload: &Workload, iterations: usize) -> Duration {
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(function(black_box(workload)));
    }
    start.elapsed()
}
fn allocation_count(function: fn(&Workload) -> Digest, workload: &Workload) -> (usize, usize) {
    ALLOCATIONS.store(0, Ordering::Relaxed);
    ALLOCATED_BYTES.store(0, Ordering::Relaxed);
    COUNTING.store(true, Ordering::Relaxed);
    black_box(function(workload));
    COUNTING.store(false, Ordering::Relaxed);
    (
        ALLOCATIONS.load(Ordering::Relaxed),
        ALLOCATED_BYTES.load(Ordering::Relaxed),
    )
}
fn stats(mut samples: Vec<f64>) -> (f64, f64, f64) {
    samples.sort_by(f64::total_cmp);
    (
        samples[samples.len() / 2],
        samples[0],
        samples[samples.len() - 1],
    )
}
fn main() {
    if let Err(error) = benchmark() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
fn benchmark() -> Result<(), String> {
    let mut samples = 7;
    let mut milliseconds = 100u64;
    let mut filter = String::new();
    let mut engine = String::from("both");
    let mut mode = String::from("strict");
    let mut validate_only = false;
    let mut perf_iterations = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--samples" => {
                samples = args
                    .next()
                    .ok_or("missing --samples value")?
                    .parse::<usize>()
                    .map_err(|e| e.to_string())?
            }
            "--ms" => {
                milliseconds = args
                    .next()
                    .ok_or("missing --ms value")?
                    .parse::<u64>()
                    .map_err(|e| e.to_string())?
            }
            "--filter" => filter = args.next().ok_or("missing --filter value")?,
            "--engine" => engine = args.next().ok_or("missing --engine value")?,
            "--mode" => mode = args.next().ok_or("missing --mode value")?,
            "--validate-only" => validate_only = true,
            "--perf-iterations" => {
                perf_iterations = Some(
                    args.next()
                        .ok_or("missing --perf-iterations value")?
                        .parse::<usize>()
                        .map_err(|e| e.to_string())?,
                )
            }
            "--help" => {
                println!("bench [--samples N] [--ms N] [--filter NAME] [--engine both|zvte|vte] [--mode strict|batch] [--validate-only] [--perf-iterations N]");
                return Ok(());
            }
            _ => return Err(format!("unknown argument {arg}")),
        }
    }
    if samples == 0 || milliseconds == 0 {
        return Err("samples and ms must be positive".into());
    }
    if !["both", "zvte", "vte"].contains(&engine.as_str()) {
        return Err("engine must be both, zvte, or vte".into());
    }
    if !["strict", "batch"].contains(&mode.as_str()) {
        return Err("mode must be strict or batch".into());
    }
    let z_function = if mode == "batch" {
        zvte_batch_once
    } else {
        zvte_once
    };
    let selected: Vec<_> = workloads()
        .into_iter()
        .filter(|w| w.name.contains(&filter))
        .collect();
    if selected.is_empty() {
        return Err(format!("no workload matches {filter:?}"));
    }
    if let Some(iterations) = perf_iterations {
        if iterations == 0
            || engine == "both"
            || selected.len() != 1
            || selected[0].name != filter
            || validate_only
        {
            return Err("--perf-iterations requires N > 0, --engine zvte|vte, an exact --filter workload name, and no --validate-only".into());
        }
        let workload = &selected[0];
        let total_bytes = workload
            .data
            .len()
            .checked_mul(iterations)
            .ok_or("total bytes overflow")?;
        let function = if engine == "zvte" {
            z_function
        } else {
            vte_once
        };
        let mut digest = Digest::default();
        let start = Instant::now();
        // No validation, calibration, warmup or invocation of the other parser.
        // Run the ordinary --validate-only preflight separately before perf.
        for _ in 0..iterations {
            digest = black_box(function(black_box(workload)));
        }
        let elapsed = start.elapsed();
        println!(
            "engine,workload,mode,iterations,total_bytes,elapsed_ns,digest,events_per_iteration"
        );
        println!(
            "{engine},{},{mode},{iterations},{total_bytes},{},{},{}",
            workload.name,
            elapsed.as_nanos(),
            digest.value,
            digest.events
        );
        return Ok(());
    }
    let zvte_version = include_str!("../../Cargo.toml")
        .lines()
        .find_map(|line| line.strip_prefix("version = "))
        .unwrap_or("unknown")
        .trim_matches('"');
    eprintln!("zvte {zvte_version} / vte 0.15.0 (default-features=false), arch={}, os={}, mode={mode}, samples={samples}, target_ms={milliseconds}",
        std::env::consts::ARCH, std::env::consts::OS);
    eprintln!("parser_bytes,zvte,{}", std::mem::size_of::<zvte::Parser>());
    eprintln!("parser_bytes,vte,{}", std::mem::size_of::<vte::Parser>());
    for workload in &selected {
        verify(&workload.data, workload.chunk).map_err(|e| format!("{}: {e}", workload.name))?;
        let expected = vte_once(workload);
        if zvte_once(workload) != expected || zvte_batch_once(workload) != expected {
            return Err(format!("{}: timed handler digest mismatch", workload.name));
        }
        let za = allocation_count(z_function, workload);
        let va = allocation_count(vte_once, workload);
        eprintln!("validated,{},bytes={},chunk={},events={},zvte_allocs={},zvte_allocated_bytes={},vte_allocs={},vte_allocated_bytes={}",
            workload.name, workload.data.len(), workload.chunk, expected.events, za.0, za.1, va.0, va.1);
    }
    if validate_only {
        return Ok(());
    }
    println!("workload,mode,engine,bytes,chunk,iterations,samples,median_ns_per_byte,min_ns_per_byte,max_ns_per_byte,MiB_per_s,speedup_vs_vte");
    for workload in &selected {
        let warm_v = run(vte_once, workload, 8);
        let warm_z = run(z_function, workload, 8);
        let ns_per_iteration = warm_v.max(warm_z).as_nanos() / 8;
        let iterations = ((u128::from(milliseconds) * 1_000_000) / ns_per_iteration.max(1))
            .clamp(1, 1_000_000) as usize;
        let bytes = workload.data.len() as f64 * iterations as f64;
        let mut z_times = Vec::with_capacity(samples);
        let mut v_times = Vec::with_capacity(samples);
        for sample in 0..samples {
            // Alternate order to reduce thermal/frequency/order bias.
            let order = if sample % 2 == 0 {
                [true, false]
            } else {
                [false, true]
            };
            for is_z in order {
                if (is_z && engine == "vte") || (!is_z && engine == "zvte") {
                    continue;
                }
                let elapsed = run(
                    if is_z { z_function } else { vte_once },
                    workload,
                    iterations,
                );
                let ns_per_byte = elapsed.as_secs_f64() * 1e9 / bytes;
                if is_z {
                    z_times.push(ns_per_byte);
                } else {
                    v_times.push(ns_per_byte);
                }
            }
        }
        let zs = if z_times.is_empty() {
            None
        } else {
            Some(stats(z_times))
        };
        let vs = if v_times.is_empty() {
            None
        } else {
            Some(stats(v_times))
        };
        for (name, measurements) in [("zvte", zs), ("vte", vs)] {
            if let Some((median, min, max)) = measurements {
                let throughput = 1e9 / median / (1024.0 * 1024.0);
                let speedup = vs.map(|v| v.0 / median).unwrap_or(f64::NAN);
                println!("{},{mode},{name},{},{},{iterations},{samples},{median:.6},{min:.6},{max:.6},{throughput:.3},{speedup:.3}", workload.name, workload.data.len(), workload.chunk);
            }
        }
    }
    Ok(())
}
