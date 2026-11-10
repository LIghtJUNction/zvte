//! Scanner-only ablation. These measurements do not represent parser throughput.
use std::hint::black_box;
use std::time::{Duration, Instant};

// Import production code directly: no copied implementation or public feature.
#[path = "../../src/scan.rs"]
mod production;

fn ascii_scalar(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .position(|&byte| !(0x20..0x80).contains(&byte))
        .unwrap_or(bytes.len())
}
fn escape_scalar(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .position(|&byte| byte == 0x1b)
        .unwrap_or(bytes.len())
}
fn osc_scalar(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .position(|&byte| byte < 0x20 || byte == b';')
        .unwrap_or(bytes.len())
}
type Scanner = fn(&[u8]) -> usize;
#[inline(never)]
fn measure(scanner: Scanner, input: &[u8], iterations: usize) -> Duration {
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(scanner(black_box(input)));
    }
    start.elapsed()
}
fn statistics(mut samples: Vec<f64>) -> (f64, f64, f64) {
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
    let mut samples = 7usize;
    let mut milliseconds = 25u64;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--samples" => {
                samples = args
                    .next()
                    .ok_or("missing --samples value")?
                    .parse::<usize>()
                    .map_err(|e| e.to_string())?;
            }
            "--ms" => {
                milliseconds = args
                    .next()
                    .ok_or("missing --ms value")?
                    .parse::<u64>()
                    .map_err(|e| e.to_string())?;
            }
            "--help" => {
                println!("scan [--samples N] [--ms N]\nProduction native-word scanner versus scalar iterator; scanner-only ablation.");
                return Ok(());
            }
            _ => return Err(format!("unknown argument {arg}")),
        }
    }
    if samples == 0 || milliseconds == 0 {
        return Err("samples and ms must be positive".into());
    }
    let scanners: [(&str, Scanner, Scanner); 3] = [
        ("ascii", production::ascii_run, ascii_scalar),
        ("escape", production::escape, escape_scalar),
        ("osc", production::osc_run, osc_scalar),
    ];
    eprintln!("scanner-only ablation: production src/scan.rs versus scalar iterator; arch={}, os={}, word_bytes={}, samples={samples}, target_ms={milliseconds}",
        std::env::consts::ARCH, std::env::consts::OS, std::mem::size_of::<usize>());
    eprintln!("ns/byte divides by input length; each fixture scans to its last byte or end; both implementations receive identical buffers");
    println!("scanner,input,engine,bytes,matched_prefix,iterations,samples,median_ns_per_byte,min_ns_per_byte,max_ns_per_byte,speedup_vs_scalar");
    for &(name, native, scalar) in &scanners {
        for length in [16usize, 64, 4096] {
            for variant in ["ascii", "control-at-end", "high-byte-at-end"] {
                let mut input = vec![b'A'; length];
                input[length - 1] = match variant {
                    "control-at-end" => 0x1b,
                    "high-byte-at-end" => 0x80,
                    _ => b'A',
                };
                let expected = scalar(&input);
                let actual = native(&input);
                if actual != expected {
                    return Err(format!(
                        "{name}/{variant}/{length}: native={actual}, scalar={expected}"
                    ));
                }
                let warm_native = measure(native, &input, 256);
                let warm_scalar = measure(scalar, &input, 256);
                let ns_per_iteration = warm_native.max(warm_scalar).as_nanos() / 256;
                let iterations = ((u128::from(milliseconds) * 1_000_000) / ns_per_iteration.max(1))
                    .clamp(1, 10_000_000) as usize;
                let total_bytes = length as f64 * iterations as f64;
                let mut native_samples = Vec::with_capacity(samples);
                let mut scalar_samples = Vec::with_capacity(samples);
                for sample in 0..samples {
                    let order = if sample % 2 == 0 {
                        [true, false]
                    } else {
                        [false, true]
                    };
                    for is_native in order {
                        let elapsed =
                            measure(if is_native { native } else { scalar }, &input, iterations);
                        let value = elapsed.as_secs_f64() * 1e9 / total_bytes;
                        if is_native {
                            native_samples.push(value);
                        } else {
                            scalar_samples.push(value);
                        }
                    }
                }
                let native_stats = statistics(native_samples);
                let scalar_stats = statistics(scalar_samples);
                for (engine, (median, min, max)) in
                    [("native-word", native_stats), ("scalar", scalar_stats)]
                {
                    let speedup = scalar_stats.0 / median;
                    println!("{name},{variant},{engine},{length},{expected},{iterations},{samples},{median:.6},{min:.6},{max:.6},{speedup:.3}");
                }
            }
        }
    }
    Ok(())
}
