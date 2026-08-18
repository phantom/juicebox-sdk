# Repository instructions

## Purpose
This repository implements the Juicebox Protocol SDK for distributed storage and recovery of secrets using PIN authentication. The core implementation is Rust, with Swift, Android, and JavaScript bridges.

## Layout
- `rust/`: Rust workspace crates, including the SDK, protocol support crates, bridges, CLI tools, and software realm runner.
- `swift/`: Swift package, CocoaPods support, FFI build script, and demo.
- `android/`: Android library, JNI build script, and Gradle project.
- `javascript/`: WebAssembly package output and JavaScript demos.

## Validation
The Rust toolchain version used by CI is 1.75.

- Build the default Rust workspace member: `cargo build`
- Test the full Rust workspace: `cargo test --workspace`
- Check Rust formatting: `cargo fmt --all -- --check`
- Lint Rust tests and workspace crates: `cargo clippy --workspace --tests -- -D warnings`
- Build and test Android after generating JNI bindings: `./android/jni.sh` then `(cd android && ./gradlew build)`
- Build the Swift FFI and run Swift tests: `swift/ffi.sh --debug --verbose --verify` then `(cd swift && swift test -v)`

Platform-specific setup and usage are documented in `rust/sdk/README.md`, `swift/README.md`, `android/README.md`, and `javascript/README.md`.
