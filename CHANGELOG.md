# Changelog

## 0.1.0

- Incremental ESC, CSI, OSC and DCS parsing with explicit inline state.
- UTF-8 recovery, C0/C1 callbacks and bounded parameters, subparameters and OSC fields.
- Zero dependencies, zero heap allocation, always `no_std`, no unsafe code in the library.
- Const-generic OSC storage, batched ASCII callbacks and overflow notification.
- Deterministic end-of-stream and reset behavior.
- Stable Rust 1.74 MSRV, cross-platform CI, Miri, independent differential tests and benchmarks.
- Manual semantic version bumps and a single-workflow crates.io / GitHub release pipeline.
