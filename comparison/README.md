# Reproduce the comparison

This separate Cargo workspace pins `vte = 0.15.0` and disables its default
features. Both parsers therefore use a 1,024-byte inline OSC buffer, 32 numeric
entries, two intermediates, and 16 OSC fields. The checked-in lockfile fixes the
reference's transitive dependencies. This workspace is unpublished and excluded
from the `zvte` package.

## Run

```sh
cargo test --manifest-path comparison/Cargo.toml --locked
cargo run --release --manifest-path comparison/Cargo.toml --locked --bin bench -- \
  --samples 7 --ms 100 > strict.csv 2> strict-metadata.txt
cargo run --release --manifest-path comparison/Cargo.toml --locked --bin bench -- \
  --mode batch --samples 7 --ms 100 > batch.csv 2> batch-metadata.txt
```

The benchmark verifies each workload's **complete event stream** before timing.
Both parsers receive identical bytes and chunk boundaries. Comparison includes
all print, execute, CSI, ESC, OSC, hook, put, and unhook callbacks, their payloads,
parameters, intermediates, and ignore/BEL flags. No events are filtered or
normalized. A mismatch stops the benchmark.

`strict` is the performance comparison with matching callback behavior. Both
implementations run the same scalar callback arithmetic and inspect the same
parameters/payloads. `batch` changes only the `zvte` text handler: it consumes
ASCII runs together. Its expanded text/control events and final digest are
checked, but its callback granularity differs. Report batch results separately;
they are not evidence for identical-callback speedups.

All workload vectors and event collectors are prepared before timing. Each
iteration constructs one parser and feeds the entire workload in its documented
chunks. Parsers persist across chunks. Digests and final parser state pass through
`std::hint::black_box`; a digest depends on every emitted scalar and parameter.
The two implementations alternate measurement order. Each table row reports the
median of the requested independent samples, observed minimum/maximum, ns/byte,
MiB/s, and the ratio `vte ns/byte / zvte ns/byte`. Calibration chooses a common
iteration count for both implementations. This measures parsing with a small
handler; an application's rendering or logging work can dominate its own total.

Metadata records `size_of::<Parser>()`, allocations, and total allocated bytes.
Allocation instrumentation surrounds an entire parse including construction;
its reporting and workload creation run outside the measured interval. The
reference's `std`/growable-OSC configuration is not substituted for the bounded
baseline.

Use an otherwise idle machine; record compiler, CPU, target options and governor.
On Linux, pin a core with `taskset -c <allowed-cpu> ...`. Do not use
`-C target-cpu=native` for one implementation only. Results from different hosts
or compiler versions are separate experiments.

## Workloads

| Name | Input | Chunk bytes |
| --- | --- | ---: |
| ascii | Printable ASCII with CR/LF | whole input |
| csi | RGB SGR, subparameters, cursor movement, private mode | whole input |
| osc | Titles, hyperlink open/close, BEL and ESC termination | whole input |
| color-log | Colored INFO/WARN log records | whole input |
| short-chunks | Same colored records | 16 |
| one-byte | Same colored records | 1 |
| dcs | DCS headers, payloads and terminators | whole input |
| unicode | Chinese, emoji, Greek, Cyrillic and accents | whole input |
| recording | Actual program output captured through a pseudoterminal | 4,096 |
| malformed | Invalid UTF-8, controls, bad parameters, cancellation | 31 |
| truncated | Colored text ending inside a CSI header | 4,096 |

Fixture generators are in `src/lib.rs`. The actual recorded bytes and their
provenance are in `fixtures/`. Truncated streams deliberately do not call
`finish`: the reference has no equivalent EOF operation.

## Reference defects and incremental tests

Two executable regressions document `vte 0.15.0` chunk-boundary defects:

1. `C2 80` together emits `execute(0x80)`, but split after `C2` emits
   `print(U+0080)`. `zvte` consistently executes this C1 control.
2. `C2 A0 20 D0 B0` together prints NBSP, space, and Cyrillic `а`. Split after
   `C2`, the reference drops the ASCII space. `zvte` preserves all three.

The targeted differential tests exhaust all 256 input bytes from every control
state, parameter saturation/overflow, OSC capacities from zero to 4,096, DCS
termination/cancellation, all splits of representative UTF-8, and 4,096 seeded
arbitrary-byte streams. For arbitrary UTF-8 boundaries, the independent oracle
receives the whole input and `zvte` receives the varied chunks; the event lists
must match **exactly**. Separate regression tests assert the reference's two
faults and the corrected, chunk-invariant `zvte` results.

Every timed workload still passes exact **same-chunk** comparison. Unicode uses
a whole-input chunk because the reference can lose text under some multibyte
chunk layouts; the incorrect reference behavior is not timed as equivalent work.

## Branch prediction and hardware counters

Build and validate first, then measure one engine and one workload per process:

```sh
cargo build --release --manifest-path comparison/Cargo.toml --locked --bin bench
comparison/target/release/bench --validate-only
perf stat -r 5 -e cycles,instructions,branches,branch-misses \
  comparison/target/release/bench --engine zvte --filter color-log --perf-iterations 10000
perf stat -r 5 -e cycles,instructions,branches,branch-misses \
  comparison/target/release/bench --engine vte --filter color-log --perf-iterations 10000
```

`--perf-iterations` is an isolated worker. It requires exactly one named workload
and one engine, performs exactly N parses, and skips validation, calibration,
allocation instrumentation and warmups. The other parser is never invoked.
Both commands process the same number of input bytes. Run the ordinary
`--validate-only` command separately first; the isolated worker deliberately
cannot establish event equivalence by itself.

Counters cover process startup, fixture construction, the selected parser's
iterations, and reporting. Use enough iterations that those fixed costs are small.
The worker reports the exact total input bytes, allowing cycles/byte and
branch-misses/byte to be computed. Divide misses by branches for the branch-miss
percentage. A host without a usable PMU must report counters as unavailable.
Timing alone cannot establish branch prediction performance. The Benchmark
workflow attempts counters and uploads either their output or the actual failure.
When physical counters are unavailable, its x86_64 job also runs Valgrind
Cachegrind branch simulation with a fixed 256 parses per workload and engine.
Simulation artifacts are explicitly separate from hardware PMU measurements;
the instrumented runtime is never used as throughput evidence.

## Native-word scanner ablation

```sh
cargo run --release --manifest-path comparison/Cargo.toml --locked --bin scan -- \
  --samples 7 --ms 25 > scanner.csv 2> scanner-metadata.txt
```

The `scan` binary imports the actual production `src/scan.rs` directly and
compares its ASCII, ESC and OSC scans with straightforward scalar byte-position
loops. Each pair receives the same 16-, 64- or 4,096-byte input: all ASCII, a
control byte at the end, or a high byte at the end. It verifies each result,
keeps input/results observable with `black_box`, alternates measurement order,
and reports median/minimum/maximum ns per input byte and the scalar/native ratio.
Both implementations use the same default compiler target flags. The production
path uses `usize` words: eight bytes on x86_64 and aarch64, native pointer width
elsewhere, with a scalar tail. It requires no optional CPU instruction set.

This experiment measures the scanner contribution only. Its speedups are not
whole-parser speedups and must not be substituted for the strict callback table.
The Benchmark workflow runs it on native x86_64 and aarch64 runners and retains
CSV and machine metadata separately for each architecture.
