# zvte 0.1.0 — reproducible benchmark and validation report

Measured 2026-11-10. The parsing implementation is revision
[`7bc64f4fa4a16175cc9998c44953e5059e0ada9e`](https://github.com/LIghtJUNction/zvte/commit/7bc64f4fa4a16175cc9998c44953e5059e0ada9e).
[CI](https://github.com/LIghtJUNction/zvte/actions/runs/42753390057) and
[native benchmarks](https://github.com/LIghtJUNction/zvte/actions/runs/42753390335)
both completed successfully. Raw CSVs, compiler/CPU metadata and counter
availability are preserved in the architecture subdirectories beside this file.

## Protocol

The oracle is registry `vte = 0.15.0`, with `default-features = false`, pinned
with its transitive dependencies in the separate comparison lockfile. Both
parsers use a 1024-byte inline OSC buffer, 16 OSC fields, 32 numeric entries and
two intermediates. Every timed fixture passes exact event-list equality with
identical input and chunk boundaries before timing. All callback arguments are
compared, including print/control order, subparameters, ignored sequences, DCS
hook/put/unhook and OSC termination flags. No events are filtered or normalized.

Strict timing uses the same scalar callback arithmetic for both implementations.
The handler consumes every character, parameter and payload. Inputs, final parser
state and digests remain observable through `black_box`. Fixture generation and
full event collection are outside timing; parser construction is included.
Each row is the median of seven alternating-order samples, targeting 100 ms per
sample for the slower implementation with the same iteration count for both.
Raw CSVs include minima, maxima, bytes, chunk size and exact iteration count.

Both builds use Rust 1.99.1, thin LTO and one codegen unit. Neither build uses
`target-cpu=native` or extra target features. GitHub's hosted runners are shared
systems; the results describe these machines and this callback workload.

## Strict same-callback throughput

Higher MiB/s and speedup are better; lower ns/byte is better. Speedup is
`vte time / zvte time`, calculated before rounding.

### x86_64 — AMD EPYC 9V74

| Workload | vte ns/B | zvte ns/B | vte MiB/s | zvte MiB/s | Speedup |
| --- | ---: | ---: | ---: | ---: | ---: |
| ascii | 0.854 | 0.380 | 1117.3 | 2507.6 | 2.244× |
| csi | 3.322 | 2.094 | 287.1 | 455.5 | 1.587× |
| osc | 3.230 | 2.157 | 295.3 | 442.2 | 1.497× |
| color-log | 1.382 | 0.762 | 690.0 | 1251.8 | 1.814× |
| short-chunks | 1.896 | 0.988 | 503.0 | 965.6 | 1.920× |
| one-byte | 7.015 | 3.951 | 136.0 | 241.4 | 1.775× |
| dcs | 3.247 | 1.702 | 293.7 | 560.2 | 1.907× |
| unicode | 1.486 | 1.523 | 641.7 | 626.2 | 0.976× |
| recording | 1.836 | 1.160 | 519.5 | 822.0 | 1.582× |
| malformed | 3.753 | 3.167 | 254.1 | 301.1 | 1.185× |
| truncated | 2.798 | 2.188 | 340.8 | 435.8 | 1.279× |

Equal-weight geometric mean across these eleven fixtures: **1.574×**. This aggregate is not weighted by application traffic.

### aarch64 — Arm Neoverse-N2

| Workload | vte ns/B | zvte ns/B | vte MiB/s | zvte MiB/s | Speedup |
| --- | ---: | ---: | ---: | ---: | ---: |
| ascii | 1.323 | 0.499 | 720.7 | 1910.3 | 2.651× |
| csi | 3.409 | 2.327 | 279.7 | 409.8 | 1.465× |
| osc | 3.640 | 2.174 | 262.0 | 438.6 | 1.674× |
| color-log | 1.860 | 0.981 | 512.7 | 972.4 | 1.897× |
| short-chunks | 2.258 | 1.229 | 422.3 | 776.0 | 1.838× |
| one-byte | 6.922 | 4.645 | 137.8 | 205.3 | 1.490× |
| dcs | 3.618 | 1.819 | 263.6 | 524.2 | 1.989× |
| unicode | 1.733 | 1.777 | 550.2 | 536.6 | 0.975× |
| recording | 2.426 | 1.511 | 393.1 | 631.1 | 1.605× |
| malformed | 3.707 | 3.575 | 257.3 | 266.7 | 1.037× |
| truncated | 2.859 | 2.296 | 333.6 | 415.4 | 1.245× |

Equal-weight geometric mean across these eleven fixtures: **1.563×**. This aggregate is not weighted by application traffic.

The strict ASCII workload exceeds the 2× target on both architectures. Other
workloads improve by varying amounts; Unicode is slightly slower. The full suite
does not reach an overall 2× or 3× result. The table retains all required workload
classes, including malformed input and truncated sequences.

The recorded workload is **23,352 bytes** of actual Git/rustc output captured via
`script` through a pseudoterminal. Its CR/LF bytes are preserved by `.gitattributes`.
The producer command and provenance are in `../fixtures/README.md`. Synthetic
workloads cover ASCII, CSI, OSC, mixed colored logs, 16-byte chunks, single-byte
chunks, DCS, Unicode, malformed streams and a stream ending inside CSI.

## Release-commit reproduction

[A second complete native run](https://github.com/LIghtJUNction/zvte/actions/runs/42755384250)
tested the published commit `5c967e05e9163ec6a554f9f599b8880a05fda0f3`.
The parser and benchmark source are unchanged from the primary measurement.
This time the x86_64 runner used **AMD EPYC 7763**; aarch64 again used
**Arm Neoverse-N2**. The same seven-sample protocol produced the following
strict speedups. These are separate runs, not pooled samples or selected maxima.

| Workload | x86_64 speedup | aarch64 speedup |
| --- | ---: | ---: |
| ascii | 2.196× | 2.649× |
| csi | 1.644× | 1.470× |
| osc | 1.095× | 1.676× |
| color-log | 2.045× | 1.887× |
| short-chunks | 2.008× | 1.850× |
| one-byte | 1.816× | 1.493× |
| dcs | 2.080× | 2.021× |
| unicode | 1.057× | 0.974× |
| recording | 1.616× | 1.612× |
| malformed | 1.113× | 1.035× |
| truncated | 1.355× | 1.241× |

The equal-weight geometric means are **1.584×** and **1.566×**.
The full results, per-byte times, CPU metadata and PMU availability are preserved
under [`release-5c967e05/`](release-5c967e05/). Differences between machines and
runs are visible, especially for OSC and Unicode; no cross-machine performance
guarantee is implied.

## Batch callbacks

The optional `Perform::text(&[u8])` callback receives printable ASCII runs. Its
default implementation invokes `print` for each byte. The batch experiment
replaces only this handler with an equivalent aggregate calculation. Its
expanded events and final digest still match, but callback granularity changes;
these results are separate from the strict comparison above.

| Architecture | ASCII zvte MiB/s | ASCII speedup vs scalar vte |
| --- | ---: | ---: |
| x86_64 | 3341.1 | 2.994× |
| aarch64 | 2528.3 | 3.496× |

Full batch measurements for all eleven workloads are in each `batch.csv`.

## Memory and implementation

On both measured 64-bit targets, `size_of::<zvte::Parser>()` is **1,248 bytes**,
compared with **1,432 bytes** for bounded `vte::Parser`: **184 bytes (12.8%) less**.
These figures include each 1024-byte OSC buffer. Both bounded implementations
record **zero allocations and zero allocated bytes** for every workload.

`zvte` always compiles with `#![no_std]` and `#![forbid(unsafe_code)]`. Its complete
production, development, build and target dependency lists are empty. No `alloc`,
`std`, FFI, `build.rs` or runtime framework is used by the library. All mutable
parser state is explicit. The shared 7,168-byte transition table is immutable.

The fast path scans native machine words (64 bits on x86_64/aarch64, pointer width
on other targets), dispatches printable ASCII in runs, batches OSC payload copies
and scans numeric headers directly. Parameter group endpoints use one 32-bit
bitmap. Long malformed UTF-8 runs search for ESC once and advance through errors,
preventing repeated full-suffix searches. Safe scalar loops handle word tails.

## Scanner ablation

The `scan` binary imports the actual production scanner module and compares it
with scalar `Iterator::position` baselines. It checks the result before timing,
uses identical default target settings and keeps inputs/results observable.
It covers all three scanners at 16, 64 and 4,096 bytes with plain ASCII, a final
control byte and a final high byte. Seven alternating samples target 25 ms.
These are scanner-only speedups, not whole-parser throughput claims. See
`scanner.csv` for all measurements and the input/engine definition.

## Branch prediction

Physical cycles, instructions, branches and branch misses were unavailable on
both hosted runners: their `perf-capability.txt` files record the exact PMU
failure. The local executor separately returned ENOENT for each hardware event
while successfully opening a software CPU-clock counter. Hardware branch
prediction performance remains unmeasured. The isolated single-engine worker
supports PMU measurements on a host that exposes these events.

### x86_64 Cachegrind 3.26.0 simulation

A separate [Benchmark run](https://github.com/LIghtJUNction/zvte/actions/runs/42754068441)
completed **Cachegrind 3.26.0 branch-predictor simulation** at source
[`b47110973020bef722f68e0d9993a097590f9abd`](https://github.com/LIghtJUNction/zvte/commit/b47110973020bef722f68e0d9993a097590f9abd).
Its parser and benchmark source match the frozen parsing revision above. Each
engine processed the identical workload bytes for exactly **256 iterations**
using the strict callback handler. The reported counts cover the isolated
single-engine process, including setup and reporting. These are modeled
conditional plus indirect branch events; they are separate from physical PMU
counters and native throughput measurements. Instrumented worker `elapsed_ns`
is excluded from all throughput claims.

The table divides total modeled branches and misses by the exact processed
input-byte count. Lower counts per byte are better within this simulation.

| Workload | vte branches/B | zvte branches/B | vte misses/B | zvte misses/B |
| --- | ---: | ---: | ---: | ---: |
| ascii | 2.127426 | 0.886788 | 0.000337 | 0.040325 |
| csi | 13.947954 | 7.781492 | 0.979693 | 0.917174 |
| osc | 11.089146 | 6.534029 | 0.856364 | 0.492127 |
| color-log | 4.755859 | 2.628816 | 0.283739 | 0.211648 |
| short-chunks | 6.366491 | 3.464190 | 0.447409 | 0.402705 |
| one-byte | 22.420358 | 14.539398 | 0.253302 | 0.244145 |
| dcs | 9.989170 | 4.663317 | 0.413573 | 0.435291 |
| unicode | 6.260155 | 6.105103 | 0.420355 | 0.390407 |
| recording | 6.634521 | 4.044636 | 0.500439 | 0.356819 |
| malformed | 12.270066 | 8.194070 | 0.787920 | 0.543723 |
| truncated | 10.709233 | 8.015360 | 0.627263 | 0.626500 |

`zvte` produces fewer modeled branches per input byte in all eleven workloads.
Its modeled misses per byte decrease in nine workloads and increase for ASCII
and DCS. The ASCII increase is substantial: **0.000337 → 0.040325 misses/byte**;
the DCS result is **0.413573 → 0.435291 misses/byte**. The miss percentage
(`misses / branches`) is higher in nine of eleven workloads because branch
counts and miss counts change by different proportions. For example, colored
logs reduce misses per byte from 0.283739 to 0.211648 while their modeled miss
percentage rises from 5.966% to 8.051%. The result is fewer modeled branches
with workload-dependent miss behavior; it supports no blanket claim of superior
branch prediction.

The unrounded counts, conditional/indirect breakdown and simulated miss
percentages are in
[`x86_64/branch-simulation/simulated-branches.csv`](x86_64/branch-simulation/simulated-branches.csv),
with the tool version and scope alongside it. The complete per-process
Cachegrind outputs remain in the run's benchmark artifact. Reproduction
commands for both PMU measurement and simulation are in `../README.md`.

## Correctness and supported behavior

The independent core suite includes 37 semantic/property tests, a scanner test,
a zero-allocation test and a compiling documentation example. It covers complete
and truncated ESC/CSI/OSC/DCS, C0/C1, UTF-8 at every split, illegal encodings,
parameter saturation/capacity, OSC capacity zero, large overflow, cancellation,
end-of-stream handling, resets and independent interleaved parsers. The comparison
suite adds ten tests, including 4,096 seeded arbitrary-byte streams and every
input byte from every control-state prefix.

Two vte 0.15.0 streaming defects have explicit regressions: split UTF-8 C1 changes
its callback type, and one split UTF-8 arrangement drops an ASCII space. `zvte`
keeps chunk-invariant events. Arbitrary streaming differential tests therefore
compare whole-input vte events with both whole and varied-chunk zvte events,
without normalizing events. All timed fixtures use identical chunks and exact
raw event equality. The Unicode timing fixture uses a whole-input chunk to avoid
measuring a reference execution that drops text.

The library follows vte's seven-bit sequence introducers. Raw C1 bytes in ground
are executed, not treated as eight-bit sequence introducers. Numeric values
saturate at u16::MAX; capacity overflow is reported. OSC overflow preserves the
bounded prefix and invokes `osc_overflow` once. These bounds are explicit API
choices, and the baseline uses the same bounds.

## Validation

| Gate | Observed result |
| --- | --- |
| Linux x86_64 / Linux aarch64 / macOS / Windows | stable fmt, clippy, tests, doc and differential tests passed |
| MSRV 1.74.0 | tests and documentation passed |
| MSRV cross builds | WASM, Cortex-M, RISC-V and aarch64 passed |
| Miri | full core suite passed with strict provenance |
| Package boundary | zero dependencies, no build.rs, comparison excluded |
| Release scripts | 15 tests passed, including bump, archive finalization and publication retry checks |
| cargo publish --dry-run | passed in CI |

## Publication result

**zvte 0.1.0 was published on 2026-11-10 at 04:23:57 UTC.**
The [Release workflow](https://github.com/LIghtJUNction/zvte/actions/runs/42755384665)
completed successfully, including its full CI gate, dry run, verified package,
crates.io upload, annotated tag and GitHub Release. The
[standalone CI](https://github.com/LIghtJUNction/zvte/actions/runs/42755384230)
also passed all eleven jobs.

- Registry: [zvte 0.1.0](https://crates.io/crates/zvte/0.1.0), not yanked at verification.
- Release: [v0.1.0](https://github.com/LIghtJUNction/zvte/releases/tag/v0.1.0), published, not a draft or prerelease.
- Annotated tag target: `5c967e05e9163ec6a554f9f599b8880a05fda0f3`.
- Published crate and Release attachment: **13,855 bytes** each.
- SHA-256, independently checked against the downloaded registry archive and GitHub asset digest:
  `1e3bb7ee938cb91c38ce7544c9446d8ed327eea9f3134c46eae558585f5b4be9`.

The published parser source is identical to the primary benchmark revision.
Manual releases support `none`, `patch`, `minor` and `major`. Version preparation,
tests, publication and tag/Release creation happen in the same trusted workflow;
PR events cannot publish. Version retries require an identical published checksum.

## Reproduce

```sh
cargo test --locked
cargo +1.74.0 test --locked
cargo +nightly miri test --locked
cargo test --manifest-path comparison/Cargo.toml --locked
cargo run --release --manifest-path comparison/Cargo.toml --locked --bin bench -- --mode strict --samples 7 --ms 100
cargo run --release --manifest-path comparison/Cargo.toml --locked --bin bench -- --mode batch --samples 7 --ms 100
cargo run --release --manifest-path comparison/Cargo.toml --locked --bin scan -- --samples 7 --ms 25
```

## Evidence review

| Question | Evidence / limit |
| --- | --- |
| Does the core preserve required syntax? | Explicit expected events, randomized chunk properties and fixed-reference differential tests |
| Are comparison claims fair? | Identical bytes/chunks/callback arithmetic/capacities; exact event preflight; all workloads reported |
| Does the optimization work on both architectures? | Native x86_64 and aarch64 parser results plus production-scanner ablation |
| Are speed claims supported? | Individual rows and raw CSV; full-suite 2×/3× claim is not made |
| Are unavailable metrics identified? | Physical PMU counters explicitly unavailable; Cachegrind simulation labeled separately with all workload results |
