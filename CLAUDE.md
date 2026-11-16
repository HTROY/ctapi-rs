# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands

```bash
# Build the entire workspace
cargo build

# Run all tests (requires Citect SCADA runtime — most tests are #[ignore]d)
cargo test

# Run a specific test (including ignored ones when connected to SCADA)
cargo test client_tag_read_ex_test -- --ignored

# Lint
cargo clippy --all-targets

# Format
cargo fmt
```

## Project Architecture

This is a Rust workspace that provides safe bindings to Citect SCADA's CtAPI (Windows-only). The workspace has three crate groups:

### ctapi-sys (low-level FFI)
- Raw `unsafe` FFI bindings to `CtAPI.dll`
- Declares C structs (`CtTagValueItems`, `CtHScale`, `CtScale`) and extern functions
- `build.rs` copies x86/x64 DLLs to the output directory at compile time
- Uses `windows-sys` for `OVERLAPPED`, `HANDLE`, `CloseHandle` types

### ctapi-rs (safe high-level API)
- **`client.rs`** — `CtClient` wraps the CtAPI connection handle (`ctOpen`/`ctClose`). Implements `Send + Sync` for `Arc`-based sharing across threads. Provides `tag_read`, `tag_read_ex`, `tag_write`, `tag_write_str`, `cicode`, `find_first`, `list_new`.
- **`find.rs`** — `CtFind` (iterator over search results) and `FindObject` (property access via `ctFindFirst`/`ctFindNext`/`ctGetProperty`). NOT `Send`/`Sync` — each thread needs its own instance. Holds a `&CtClient` reference and must be dropped before the client.
- **`list.rs`** — `CtList` manages tag lists for batch read/write via `ctListNew`/`ctListAdd`/`ctListRead`/etc. Holds an `Arc<CtClient>` and is guards its tag map with an internal RwLock, making it `Send + Sync`. Can be shared across threads via `Arc<CtList>`.
- **async_ops.rs** — AsyncOperation and Future traits run blocking calls on owned workers.
- **tokio_async.rs** — TokioCtClient and TokioCtList use spawn_blocking behind tokio-support.
- **`scaling.rs`** — Engineering unit↔raw value conversion.
- **`error.rs`** — `CtApiError` enum using `thiserror`.
- **`constants.rs`** — CtAPI constants (`CT_OPEN_RECONNECT`, buffer sizes, etc.).
- **`lib.rs`** — Re-exports all public types, traits, and `anyhow::Result`.

### examples/
- `client` — basic connection and tag operations
- `list-read` — tag list batch operations
- `async-demo` — Worker-based async usage
- `tokio-demo` — Tokio async/await (requires `--features tokio-support`)

## Key Design Decisions

- **GBK encoding**: Citect SCADA uses GBK. Every string parameter is GBK-encoded before FFI; every response buffer is GBK-decoded via `encoding_rs::GBK`.
- **`tag_write` vs `tag_write_str`**: `tag_write` requires `Display + Add + Sub + Copy` (numeric types). `tag_write_str` accepts any `&str`. The raw FFI is the same — use whichever matches your caller.
- **Async lifetime**: See ASYNC_IMPLEMENTATION.md when changing worker ownership, result collection, or cancellation.
- **Thread safety**: `CtClient` and `CtList` are both `Send + Sync`. `CtClient` is safe because CtAPI.dll is documented thread-safe. `CtList` uses an RwLock to guard its tag map. `CtFind` borrows `&CtClient` and is NOT `Send`/`Sync` — each thread needs its own instance.
See TEST_CONFIGURATION.md for the SCADA fixture environment.
- **Client ownership**: CtClient clones share one Rust owner that closes the native handle after the last clone drops.

## Changelog

Uses `git-cliff` with conventional commits and `cliff.toml`: just run `git-cliff -o CHANGELOG.md` after tagging.
