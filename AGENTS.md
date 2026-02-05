# AGENTS.md - rmx Development Guide

> Fast parallel directory deletion for Windows using POSIX semantics

## Quick Reference

```bash
# Build
cargo build                          # Debug build
cargo build --release                # Release build (optimized)

# Test
cargo test                           # Run all tests
cargo test <test_name>               # Run single test (e.g., cargo test test_dry_run)
cargo test --test integration_tests  # Run integration tests only
cargo test --test stress_tests       # Run stress tests only
cargo test --test concurrency_tests  # Run concurrency tests only

# Lint/Format
cargo fmt                            # Format code
cargo fmt --check                    # Check formatting
cargo clippy                         # Run linter
cargo clippy -- -D warnings          # Treat warnings as errors

# Run
cargo run -- -rf ./target            # Delete directory
cargo run -- -n ./node_modules       # Dry run
cargo run -- --help                  # Show help
```

## Project Structure

```
rmx/
├── src/
│   ├── main.rs          # CLI entry point, argument parsing (clap)
│   ├── lib.rs           # Public module exports
│   ├── broker.rs        # Work distribution for parallel deletion
│   ├── worker.rs        # Worker threads for file/directory deletion
│   ├── tree.rs          # Directory tree discovery and traversal
│   ├── winapi.rs        # Windows API wrappers (POSIX delete, etc.)
│   ├── safety.rs        # Path safety checks (system dirs, etc.)
│   ├── error.rs         # Custom error types
│   ├── context_menu.rs  # Windows Explorer integration (Windows only)
│   └── progress_ui.rs   # GUI progress window (gpui, Windows only)
├── tests/
│   ├── integration_tests.rs  # CLI integration tests
│   ├── stress_tests.rs       # Performance/load tests
│   └── concurrency_tests.rs  # Parallel execution tests
├── bucket/              # Scoop manifest
└── .github/workflows/   # CI/CD (release.yml)
```

## Code Style Guidelines

### Imports
- Group imports: std first, then external crates, then crate modules
- Use explicit imports, avoid glob imports (`use foo::*`)
- Conditional imports with `#[cfg(windows)]` for platform-specific code

```rust
use std::path::PathBuf;
use std::sync::Arc;

use crossbeam_channel::Receiver;
use dashmap::DashMap;

use crate::error::Error;
use crate::tree::DirectoryTree;
```

### Error Handling
- Use custom `Error` enum in `src/error.rs` for library errors
- Implement `Display`, `std::error::Error`, and `From` traits
- Return `Result<T, Error>` for fallible operations
- Use `.map_err()` to add context to errors

```rust
// Good
pub fn process_file(path: &Path) -> Result<(), Error> {
    do_something(path).map_err(|e| Error::io_with_path(path.to_path_buf(), e))?;
    Ok(())
}

// Avoid: unwrap/expect in library code
```

### Conditional Compilation
- Use `#[cfg(windows)]` / `#[cfg(not(windows))]` for platform-specific code
- Keep platform-specific modules guarded in `lib.rs`

```rust
// In lib.rs
#[cfg(windows)]
pub mod context_menu;

// In implementation
#[cfg(windows)]
fn windows_specific() { ... }

#[cfg(not(windows))]
fn windows_specific() {
    // Stub or error
}
```

### Naming Conventions
- Types: `PascalCase` (e.g., `DirectoryTree`, `WorkerConfig`)
- Functions/methods: `snake_case` (e.g., `delete_file`, `mark_complete`)
- Constants: `SCREAMING_SNAKE_CASE` (e.g., `APP_VERSION`)
- Modules: `snake_case` (e.g., `progress_ui`)

### Documentation
- Doc comments (`///`) for public APIs
- Inline comments for non-obvious logic
- Chinese comments acceptable for internal notes

```rust
/// Deletes a file using POSIX semantics for immediate namespace removal.
///
/// # Arguments
/// * `path` - The path to delete
///
/// # Returns
/// * `Ok(())` on success
/// * `Err` with IO error on failure
pub fn delete_file(path: &Path) -> std::io::Result<()> { ... }
```

### Struct Design
- Implement `Default` where sensible
- Use `#[derive(...)]` for common traits
- Public fields for simple data structs

```rust
#[derive(Clone)]
pub struct WorkerConfig {
    pub verbose: bool,
    pub ignore_errors: bool,
    pub kill_processes: bool,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            verbose: false,
            ignore_errors: true,
            kill_processes: false,
        }
    }
}
```

### Concurrency
- Use `Arc<T>` for shared ownership across threads
- Prefer `parking_lot::Mutex` over `std::sync::Mutex`
- Use `DashMap` for concurrent hash maps
- Use `AtomicUsize`/`AtomicBool` for simple counters/flags
- Use `crossbeam-channel` for worker communication

### Testing
- Integration tests in `tests/` directory
- Helper functions at top of test files
- Use descriptive test names: `test_<feature>_<scenario>`
- Clean up test artifacts after tests

```rust
#[test]
fn test_dry_run_does_not_delete() {
    let test_dir = create_test_dir("dry_run");
    // ... test logic
    fs::remove_dir_all(&test_dir).ok(); // Cleanup
}
```

## Windows API Patterns

This project uses low-level Windows APIs for performance:
- `CreateFileW` with `FILE_SHARE_DELETE`
- `SetFileInformationByHandle` with `FILE_DISPOSITION_INFORMATION_EX`
- `FILE_DISPOSITION_POSIX_SEMANTICS` for immediate removal
- Long path support with `\\?\` prefix

## Build Configuration

### Release Profile (Cargo.toml)
```toml
[profile.release]
opt-level = 3
lto = true
codegen-units = 1
panic = 'abort'
strip = true
```

### Version Handling
- `build.rs` sets `APP_VERSION` from `CI_VERSION` env var (CI) or `CARGO_PKG_VERSION`
- Access via `env!("APP_VERSION")` in code

## Test Baseline (Performance Regression Reference)

Last verified: 2025-02-05

| Test Suite | Tests | Status | Duration |
|------------|-------|--------|----------|
| Unit tests | 2 | PASS | 0.01s |
| Concurrency tests | 12 | PASS | ~16s |
| Integration tests | 16 | PASS | ~2s |
| Stress tests | 8 (+1 ignored) | PASS | ~38s |

Key test scenarios:
- `concurrency_high_contention`: Parallel deletion under heavy load
- `concurrency_thread_scaling`: Thread count scaling validation
- `stress_test_node_modules_medium`: node_modules simulation
- `stress_test_deep_nesting`: Deep directory structures

## Important Notes

1. **Windows Only**: Core deletion features require Windows 10 1607+
2. **Safety First**: Never bypass safety checks in `safety.rs`
3. **No Force Push**: Git operations require explicit user request
4. **Test Before PR**: Run `cargo test && cargo clippy` before submitting
