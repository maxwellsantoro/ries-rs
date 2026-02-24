# Contributing to RIES-RS

Thank you for your interest in contributing to RIES-RS! This document provides guidelines and instructions for contributing.

## Development Setup

### Prerequisites

- Rust 1.70 or later (install via [rustup](https://rustup.rs/))
- For `highprec` feature: GMP and MPFR libraries
  - Ubuntu/Debian: `sudo apt-get install libgmp-dev libmpfr-dev`
  - macOS: `brew install gmp mpfr`
  - Windows: Use MSYS2 or WSL

### Building

```bash
# Standard build
cargo build

# With all optional features
cargo build --all-features

# Python bindings (requires maturin)
cd ries-py
maturin develop --release
```

> **Note:** The Python bindings live in `ries-py/`. If you just want type-checking
> without building a wheel, you can run:
>
> `cargo check --manifest-path ries-py/Cargo.toml`

### Testing

```bash
# Run all tests
cargo test

# Run tests with highprec feature
cargo test --features highprec

# Run specific test file
cargo test --test integration_tests

# Run property tests only
cargo test --test property_tests
```

## Code Style

We use standard Rust conventions:

```bash
# Format code
cargo fmt

# Check for linting issues
cargo clippy --all-targets -- -D warnings
```

### Key Conventions

1. **Documentation**: All public APIs must have rustdoc comments
2. **Error handling**: Use `thiserror` for error types
3. **Testing**: New features require unit tests; mathematical features require property tests
4. **Commits**: Follow conventional commit format (`feat:`, `fix:`, `docs:`, `refactor:`)

## Project Structure

```
src/
├── lib.rs          # Library entry point, re-exports
├── main.rs         # CLI binary entry
├── cli/            # CLI argument parsing and output
├── expr.rs         # Expression representation
├── eval.rs         # Expression evaluation with AD
├── gen.rs          # Expression generation
├── search.rs       # Search and matching
├── symbol.rs       # Symbol definitions
└── thresholds.rs   # Named constants
```

## Feature Flags

| Flag | Description | Dependencies |
|------|-------------|--------------|
| `parallel` | Multi-threaded search (default) | rayon |
| `highprec` | Arbitrary precision arithmetic | rug, GMP, MPFR |
| `wasm` | WebAssembly bindings | wasm-bindgen |

## Pull Request Process

1. Fork the repository and create a feature branch
2. Make your changes with appropriate tests
3. Ensure all CI checks pass:
   - `cargo fmt -- --check`
   - `cargo clippy --all-targets -- -D warnings`
   - `cargo test --tests`
4. Update documentation if changing public APIs
5. Submit PR with a clear description of changes

## Running Benchmarks

```bash
cargo bench
```

## Questions?

Open an issue for bugs, feature requests, or questions about the codebase.
