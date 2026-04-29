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
- **`list.rs`** — `CtList` manages tag lists for batch read/write via `ctListNew`/`ctListAddTag`/`ctListRead`/etc. Holds a `&CtClient` reference, NOT `Send`/`Sync`.
- **`async_ops.rs`** — Three layers of async: `AsyncOperation` (OVERLAPPED handle), `AsyncCtClient` trait (callback-style), `CtApiFuture` (std `Future` with a waker thread), and `FutureCtClient` trait (returns `CtApiFuture` for `.await`).
- **`tokio_async.rs`** — `TokioCtClient` (cicode/tag_read/tag_write via `spawn_blocking`), `TokioCtList` (OVERLAPPED read/write with polling). Feature-gated behind `tokio-support`.
- **`scaling.rs`** — Engineering unit↔raw value conversion.
- **`error.rs`** — `CtApiError` enum using `thiserror`.
- **`constants.rs`** — CtAPI constants (`CT_OPEN_RECONNECT`, buffer sizes, etc.).
- **`lib.rs`** — Re-exports all public types, traits, and `anyhow::Result`.

### examples/
- `client` — basic connection and tag operations
- `list-read` — tag list batch operations
- `async-demo` — OVERLAPPED-based async usage
- `tokio-demo` — Tokio async/await (requires `--features tokio-support`)

## Key Design Decisions

- **GBK encoding**: Citect SCADA uses GBK. Every string parameter is GBK-encoded before FFI; every response buffer is GBK-decoded via `encoding_rs::GBK`.
- **`tag_write` vs `tag_write_str`**: `tag_write` requires `Display + Add + Sub + Copy` (numeric types). `tag_write_str` accepts any `&str`. The raw FFI is the same — use whichever matches your caller.
- **Two async models**: `FutureCtClient` (OVERLAPPED-based, no blocking thread — ideal for Cicode) and `TokioCtClient` (spawn_blocking — needed for tag_read/write which don't support OVERLAPPED). `TokioCtList` uses OVERLAPPED with polling.
- **Thread safety**: `CtClient` is `Send + Sync` (CtAPI.dll is documented thread-safe for reads). `CtFind` and `CtList` borrow from `CtClient` and are neither — each thread gets its own.
- **Tests use env vars**: `CITECT_COMPUTER`, `CITECT_USER`, `CITECT_PASSWORD` for connection params. All integration tests are `#[ignore]`d by default since they need a live SCADA system.
- **CtClient derives Clone + PartialEq**: cloning increments an internal CtAPI reference count (same underlying handle). `Drop` calls `ctClose`. The `PartialEq` compares raw handles.

## Changelog

Uses `git-cliff` with conventional commits and `cliff.toml`: just run `git-cliff -o CHANGELOG.md` after tagging.
