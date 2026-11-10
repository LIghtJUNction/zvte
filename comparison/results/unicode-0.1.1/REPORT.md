# Unicode optimization for zvte 0.1.1

Measured 2026-11-10 at parsing revision
[`a2368c7236ddf179631eff7670c377bfdd46f208`](https://github.com/LIghtJUNction/zvte/commit/a2368c7236ddf179631eff7670c377bfdd46f208).
[CI](https://github.com/LIghtJUNction/zvte/actions/runs/42786686812) and
[native benchmarks](https://github.com/LIghtJUNction/zvte/actions/runs/42786687036)
completed successfully. Measurement preceded the release-version bump, so the
unchanged raw metadata reports package version 0.1.0; the commit identifies the
optimized implementation evaluated here.

## Result and scope

The original mixed-script `unicode` workload now exceeds `vte 0.15.0` on both
measured hosts: **1.561× on x86_64** and **1.250× on aarch64**, with the same
per-character callbacks. Its input bytes, repetition count and whole-input chunk
boundary are unchanged from the [0.1.0 experiment](../REPORT.md). Six additional
Unicode workloads check encoding widths, varied terminal records and incremental
input. All seventeen workloads remain in the tables below, including malformed
input and the previously required non-Unicode cases.

The x86_64 host is now **Intel Xeon 6973P-C**, whereas the earlier 0.1.0 report
used AMD EPYC hosts. Absolute old/new throughput values therefore do not form a
controlled same-host comparison. Each speedup in this report compares both
parsers within the same new run. The aarch64 host is **Arm Neoverse-N2**. These
results describe the measured hosts, compiler and callback workload; they are not
a guarantee for every CPU or Unicode stream. The aarch64 two-byte workload's
**1.003×** median is effectively parity and is not treated as a substantial gain.

## Why the implementation changed

The prior valid-Unicode path inspected the same input for several purposes:
finding ESC, validating UTF-8 and then decoding characters for callbacks. The
new `start_utf8` loop validates each complete scalar and assembles its value
together. It recognizes ESC when examining an ASCII byte and returns control to
the existing VT parser. This removes separate ESC and UTF-8 validation passes
from the complete valid-scalar path without changing emitted events.

The decoder rejects invalid encodings before converting a scalar. It first
checks that the required bytes are available, then applies these ranges:

| Leading byte | Second byte | Remaining continuation bytes |
| --- | --- | --- |
| C2–DF | 80–BF | none |
| E0 | A0–BF | 80–BF |
| E1–EC, EE–EF | 80–BF | 80–BF |
| ED | 80–9F | 80–BF |
| F0 | 90–BF | 80–BF |
| F1–F3 | 80–BF | 80–BF |
| F4 | 80–8F | 80–BF |

These checks exclude overlong encodings, surrogate code points and values above
U+10FFFF. Conversion still uses safe `char::from_u32`. The existing dispatch
function continues to emit C0/C1 through `execute` and other scalars through
`print`. An invalid or truncated suffix uses the established recovery path,
preserving invalid-byte grouping, replacement characters and pending state.

Split scalars now validate only newly received bytes. A nonzero buffered length
means the stored bytes already form a valid incomplete prefix. Incoming bytes
use the same restricted second-byte ranges and continuation checks. On an invalid
byte, the parser emits one replacement for the preceding prefix, clears the
partial state and leaves the offending byte for normal ground-state handling.
On a short chunk it retains the valid prefix; on completion it emits the scalar
once. This avoids repeatedly validating the buffered prefix during single-byte
input while preserving controls that interrupt a scalar.

The change adds no fields, allocation or unsafe code. The published core remains
`no_std`, uses only `core`, has no third-party dependencies and requires no
optional CPU instruction set. Stable Rust 1.74.0 remains the MSRV.

## Correctness evidence

The core suite contains **39 semantic/property tests**, plus scanner,
zero-allocation and documentation tests. New coverage round-trips every valid
Unicode scalar from U+0020 through U+10FFFF, excluding the surrogate range, with
both whole-input and 17-byte chunks. It checks exact scalar order and the distinct
C1 execution callback. Miri uses a smaller sample retaining encoding-width
boundaries, both sides of the surrogate gap, the maximum scalar and representative
scripts. Explicit malformed cases check overlong encodings, surrogates,
out-of-range values, incomplete prefixes and ESC interruptions at every pair of
split positions.

The comparison suite contains **11 tests**. Its new boundary test tries all 256
next bytes after thirteen UTF-8 prefixes, including the E0/ED/F0/F4 restrictions,
at every split. Existing tests retain 4,096 seeded arbitrary-byte streams,
parameter limits, OSC/DCS behavior and recovery across control states. The full
native, MSRV, cross-target and Miri gates passed in the linked CI run.

Two known reference defects remain explicit: split UTF-8 C1 can change its
callback type, and a particular split can cause `vte 0.15.0` to drop an ASCII
space. Arbitrary-stream differential tests compare whole-input reference events
with varied-chunk `zvte` events without normalizing either list. Every timed
workload separately requires exact equality under identical chunks. The 17-byte
CJK and single-byte emoji fixtures pass that stricter preflight and do not
trigger either defect; their results do not imply the reference is correct for
all possible Unicode chunk layouts.

## Measurement protocol

The independent comparison workspace pins registry `vte = 0.15.0` with default
features disabled. Both parsers use a 1,024-byte OSC buffer, 16 OSC fields,
32 numeric entries and two intermediate bytes. The same callback arithmetic
consumes every emitted scalar, parameter and payload. Full event-list equality
is checked before timing. Inputs, parser state and final digests stay observable
through `black_box`; no events are filtered or normalized.

Each row is the median of **seven alternating-order samples**, targeting
**100 ms** per sample for the slower implementation with a common iteration
count. Parser construction is included; fixture construction and full event
collection are outside timing. Both builds use Rust **1.99.1**, thin LTO and one
codegen unit, with no `target-cpu=native` or extra target features. Raw CSVs retain
sample minima/maxima, iteration counts, input lengths and chunk sizes.

The original eleven workloads are retained unchanged. Added whole-input cases
cover three-byte CJK, two-byte Greek/Cyrillic/Arabic/Hebrew/accented characters,
four-byte emoji and numbered mixed-script colored logs. The CJK input is also
fed in 17-byte chunks, and the emoji input one byte at a time. See
[`comparison/README.md`](../../README.md) for names, contents and filtering rules.
Original-eleven and expanded-seventeen aggregates are reported separately.

## Strict same-callback throughput

Higher MiB/s and speedup are better; lower ns/byte is better. Speedup is
`vte time / zvte time`. All rows below are from strict scalar callbacks. Separate
`batch.csv` files preserve the optional batched-ASCII experiment; their changed
callback granularity is not used for the Unicode speedup claim.

### x86_64 — Intel Xeon 6973P-C

[Raw strict measurements](x86_64/strict.csv) and [machine metadata](x86_64/environment.txt).

| Workload | vte ns/B | zvte ns/B | vte MiB/s | zvte MiB/s | Speedup |
| --- | ---: | ---: | ---: | ---: | ---: |
| ascii | 0.642 | 0.325 | 1485.9 | 2935.8 | 1.976× |
| csi | 3.230 | 1.872 | 295.3 | 509.5 | 1.726× |
| osc | 2.420 | 2.067 | 394.1 | 461.3 | 1.171× |
| color-log | 1.387 | 0.724 | 687.7 | 1317.3 | 1.915× |
| short-chunks | 1.911 | 1.066 | 499.1 | 894.7 | 1.793× |
| one-byte | 6.847 | 3.642 | 139.3 | 261.9 | 1.880× |
| dcs | 2.593 | 1.396 | 367.8 | 683.3 | 1.858× |
| unicode | 1.381 | 0.884 | 690.7 | 1078.3 | 1.561× |
| unicode-cjk | 0.829 | 0.663 | 1150.7 | 1438.7 | 1.250× |
| unicode-two-byte | 0.877 | 0.698 | 1087.8 | 1365.6 | 1.255× |
| unicode-emoji | 0.637 | 0.593 | 1496.2 | 1608.6 | 1.075× |
| unicode-mixed | 1.797 | 1.051 | 530.8 | 907.4 | 1.709× |
| unicode-short-chunks | 1.645 | 1.379 | 579.6 | 691.3 | 1.193× |
| unicode-one-byte | 7.573 | 3.356 | 125.9 | 284.1 | 2.256× |
| recording | 1.873 | 1.253 | 509.2 | 761.1 | 1.495× |
| malformed | 3.109 | 3.051 | 306.7 | 312.6 | 1.019× |
| truncated | 2.610 | 2.044 | 365.3 | 466.5 | 1.277× |

Equal-weight geometric mean: **1.572× across the original eleven** and
**1.512× across all seventeen**. Both use the CSV median ns/B ratios,
before rounding displayed speedups. Neither aggregate is weighted by application traffic.

### aarch64 — Arm Neoverse-N2

[Raw strict measurements](aarch64/strict.csv) and [machine metadata](aarch64/environment.txt).

| Workload | vte ns/B | zvte ns/B | vte MiB/s | zvte MiB/s | Speedup |
| --- | ---: | ---: | ---: | ---: | ---: |
| ascii | 1.322 | 0.480 | 721.5 | 1985.8 | 2.752× |
| csi | 3.464 | 2.342 | 275.3 | 407.2 | 1.479× |
| osc | 3.548 | 2.212 | 268.8 | 431.1 | 1.604× |
| color-log | 1.878 | 0.941 | 507.8 | 1014.0 | 1.997× |
| short-chunks | 2.286 | 1.209 | 417.2 | 788.8 | 1.891× |
| one-byte | 6.980 | 4.651 | 136.6 | 205.0 | 1.501× |
| dcs | 3.612 | 1.821 | 264.0 | 523.7 | 1.983× |
| unicode | 1.762 | 1.409 | 541.4 | 676.8 | 1.250× |
| unicode-cjk | 1.177 | 1.095 | 810.1 | 871.1 | 1.075× |
| unicode-two-byte | 1.238 | 1.234 | 770.3 | 773.0 | 1.003× |
| unicode-emoji | 0.927 | 0.881 | 1029.1 | 1082.1 | 1.051× |
| unicode-mixed | 2.231 | 1.545 | 427.4 | 617.1 | 1.444× |
| unicode-short-chunks | 2.058 | 1.780 | 463.3 | 535.8 | 1.156× |
| unicode-one-byte | 7.005 | 4.492 | 136.1 | 212.3 | 1.560× |
| recording | 2.461 | 1.470 | 387.6 | 648.8 | 1.674× |
| malformed | 3.631 | 3.480 | 262.7 | 274.0 | 1.043× |
| truncated | 2.856 | 2.243 | 333.9 | 425.2 | 1.273× |

Equal-weight geometric mean: **1.622× across the original eleven** and
**1.457× across all seventeen**. Both use the CSV median ns/B ratios,
before rounding displayed speedups. Neither aggregate is weighted by application traffic.

## Memory and allocation

Both measured 64-bit architectures report **1,248 bytes** for `zvte::Parser` and
**1,432 bytes** for bounded `vte::Parser`, including their 1,024-byte OSC buffers.
The new Unicode paths do not increase the parser layout. Both engines record
**zero allocations and zero allocated bytes in all seventeen workloads**.
The metadata files beside the raw CSVs retain these measurements and the exact
event count verified for each input.

## Branch prediction evidence

Physical cycles, instructions, branches and branch misses were unavailable on
both hosted runners. Their `perf-capability.txt` files retain the failure and
`perf_event_paranoid = 4`; no physical branch-miss rate is inferred from native
throughput. The benchmark exposes an isolated worker for systems with an
accessible PMU.

The x86_64 run also completed **Cachegrind 3.26.0 simulation** for the original
eleven workloads, using the same bytes and **256 iterations per engine**. It
models conditional and indirect branches. Process-wide counts include fixture
setup and reporting; instrumented worker elapsed time is not a native throughput
measurement. The six added Unicode fixtures are covered by native timing and
event preflight, while the simulation covers the original `unicode` case.

The table gives modeled counts per processed input byte.

| Workload | vte branches/B | zvte branches/B | vte misses/B | zvte misses/B |
| --- | ---: | ---: | ---: | ---: |
| ascii | 2.161401 | 0.920763 | 0.001049 | 0.041038 |
| csi | 14.001029 | 7.834566 | 0.980809 | 0.939122 |
| osc | 11.132326 | 6.577208 | 0.891104 | 0.611694 |
| color-log | 4.783491 | 2.656448 | 0.284319 | 0.212239 |
| short-chunks | 6.394126 | 3.491824 | 0.427666 | 0.394649 |
| one-byte | 22.447987 | 14.567027 | 0.253883 | 0.277245 |
| dcs | 10.044553 | 4.718699 | 0.436477 | 0.436454 |
| unicode | 6.294136 | 3.800763 | 0.421068 | 0.201079 |
| recording | 6.746306 | 4.156421 | 0.503688 | 0.359340 |
| malformed | 12.335429 | 8.253498 | 0.795556 | 0.576575 |
| truncated | 10.788878 | 8.095006 | 0.628937 | 0.628174 |

For the original Unicode input, modeled branches fall from 123,747,741 to
74,726,046, and modeled misses from 8,278,538 to 3,953,365. The modeled miss
percentage changes from 6.690% to 5.290%. Other workloads retain unfavorable
results: ASCII and the original single-byte colored-log workload have more
modeled misses. Lower branch count alone does not establish better prediction.

[Unrounded counts and conditional/indirect breakdown](x86_64/branch-simulation/simulated-branches.csv)
are retained with the tool version and scope. Complete per-process profiles are
available in the linked benchmark run artifact.

## Reproduce

```sh
cargo test --locked
cargo +1.74.0 test --locked
cargo +nightly miri test --locked
cargo test --manifest-path comparison/Cargo.toml --locked
cargo run --release --manifest-path comparison/Cargo.toml --locked --bin bench -- \
  --mode strict --filter unicode --samples 7 --ms 100
cargo run --release --manifest-path comparison/Cargo.toml --locked --bin bench -- \
  --mode strict --samples 7 --ms 100
```

Normal `--filter unicode` selects all seven Unicode names. Isolated
`--perf-iterations N --engine zvte --filter unicode` selects only the exact
original workload; use `--filter unicode-cjk` for that exact fixture. Run
`--validate-only` separately before isolated profiling, since that mode omits
event collection, calibration and invocation of the other engine.

## Published 0.1.1 verification

Version **0.1.1** was published on **2026-11-10 at 17:08:17 (Asia/Taipei)**.
The [Release workflow](https://github.com/LIghtJUNction/zvte/actions/runs/42787910555)
and [release-commit CI](https://github.com/LIghtJUNction/zvte/actions/runs/42787910231)
completed successfully. The registry archive, its recorded checksum and the
GitHub Release attachment were independently compared.

- [crates.io version](https://crates.io/crates/zvte/0.1.1), not yanked at verification.
- [GitHub Release and annotated tag](https://github.com/LIghtJUNction/zvte/releases/tag/v0.1.1), published, not a draft or prerelease.
- Tag target: `17c87bf66469cfd05cc505f6e62c0d96a90b5092`.
- Archive size: **15,188 bytes**.
- SHA-256: `f9aa6ef17b77c82f0b77f93cc4a31015848097622997a3b40acaf4d843928933`.
- [Published API documentation](https://docs.rs/zvte/0.1.1/zvte/) was also checked successfully.

The release commit changes only version metadata, documentation and recorded
results relative to the measured parser. Its parser source is identical to
`a2368c7`. A [complete native repeat](https://github.com/LIghtJUNction/zvte/actions/runs/42787910395)
also passed at the actual published commit. This time x86_64 used **AMD EPYC 7763**;
aarch64 again used **Arm Neoverse-N2**. Each entry below is the within-run strict
speedup against vte; these independent runs are not pooled with the Intel run.

| Workload | AMD x86_64 speedup | aarch64 speedup |
| --- | ---: | ---: |
| ascii | 2.168× | 2.752× |
| csi | 1.856× | 1.476× |
| osc | 1.125× | 1.697× |
| color-log | 1.964× | 2.000× |
| short-chunks | 1.772× | 1.892× |
| one-byte | 1.586× | 1.497× |
| dcs | 2.250× | 1.989× |
| unicode | 1.462× | 1.248× |
| unicode-cjk | 1.115× | 1.072× |
| unicode-two-byte | 1.289× | 1.003× |
| unicode-emoji | 1.136× | 1.051× |
| unicode-mixed | 1.541× | 1.454× |
| unicode-short-chunks | 1.115× | 1.142× |
| unicode-one-byte | 2.044× | 1.559× |
| recording | 1.648× | 1.677× |
| malformed | 1.155× | 1.048× |
| truncated | 1.282× | 1.273× |

All seventeen medians are numerically above vte in this repeat; the ARM two-byte
case remains effectively parity. The original Unicode workload reaches
**678.089 MiB/s on AMD** and **676.475 MiB/s on ARM**. Both engines still allocate
zero bytes. Raw times, minima/maxima, complete strict/batch/scanner results and
metadata are retained in [`release-17c87bf6/`](release-17c87bf6/). Earlier interim
measurements are not substituted for these published-code results.

## Evidence review

| Review question | Evidence and limit |
| --- | --- |
| Contribution: did the requested workload improve? | Original unchanged Unicode fixture exceeds vte in the two native runs; no universal CPU claim |
| Clarity: can the mechanism and protocol be reproduced? | Complete byte-range invariants, source revision, commands, fixed oracle and raw CSVs |
| Experimental strength: are comparisons equivalent? | Identical bytes/chunks/bounds/scalar callbacks and exact event preflight; all workloads retained |
| Evaluation completeness: are harder cases included? | Separate encoding widths, mixed records, short/single-byte chunks, malformed streams and non-Unicode workloads |
| Design soundness: does optimization retain safety and semantics? | Checked indexing/conversion, safe-only core, exhaustive valid scalars, targeted invalid cases, differential tests and Miri |
| Are important limits visible? | Different historical x86 CPU, near-parity aarch64 two-byte result, unavailable physical PMU counters and separately labeled simulation |

Historical 0.1.0 data remain unchanged in the parent results directory. These
new tables neither replace those measurements nor combine samples from different
machines into one result.
