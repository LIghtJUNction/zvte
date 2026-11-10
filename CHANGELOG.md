# Changelog

## 0.1.1

- Fuse valid UTF-8 validation, scalar decoding and callbacks into one pass.
- Validate only newly received continuation bytes for split Unicode scalars,
  and buffer single-byte UTF-8 leads directly.
- Preserve callback order, error recovery, no_std/core-only operation, zero
  dependencies, zero allocations, Parser layout and the Rust 1.74 MSRV.
- Exhaustively test valid Unicode scalars and restricted UTF-8 boundaries;
  add six supplementary Unicode benchmarks while retaining the original fixture.
- Match exact workload names in isolated perf workers when Unicode names share
  a prefix; normal benchmark filters retain substring matching.

## 0.1.0

- Incremental ESC, CSI, OSC and DCS parsing with explicit inline state.
- UTF-8 recovery, C0/C1 callbacks and bounded parameters, subparameters and OSC fields.
- Zero dependencies, zero heap allocation, always `no_std`, no unsafe code in the library.
- Const-generic OSC storage, batched ASCII callbacks and overflow notification.
- Deterministic end-of-stream and reset behavior.
- Stable Rust 1.74 MSRV, cross-platform CI, Miri, independent differential tests and benchmarks.
- Manual semantic version bumps and a single-workflow crates.io / GitHub release pipeline.
