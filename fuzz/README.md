# Fuzz targets

Deterministic robustness corpora run in normal CI
(`crates/bicmath-engine/tests/robustness.rs`). These libFuzzer targets add
coverage-guided fuzzing for maintainers:

```sh
cargo install cargo-fuzz
cargo +nightly fuzz run expression
cargo +nightly fuzz run wire_value
cargo +nightly fuzz run batch
```

The targets require a nightly toolchain and are excluded from the main
workspace so ordinary builds and CI do not depend on libFuzzer.
