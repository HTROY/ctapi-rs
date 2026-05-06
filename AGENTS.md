# AGENTS.md

## Commands

```bash
cargo build                    # Build workspace
cargo test                    # Run tests (requires Citect SCADA)
cargo test -- --ignored     # Run integration tests
cargo run --example client   # Run specific example
cargo run --example list-read
cargo run --example tokio-demo
```

## Architecture

- **ctapi-sys**: FFI bindings to CtAPI.dll (Windows-only)
- **ctapi-rs**: Safe Rust API wrapper
- **examples/**: Standalone examples

## Thread Safety

- `CtClient`: `Send + Sync` - safe to share across threads via `Arc`
- `CtFind`: NOT `Send/Sync` - each thread must create its own `CtFind` instance
- `CtList`: `Send + Sync` - can be shared across threads via `Arc<CtList>`
- Must destroy derived objects before dropping `CtClient`

## Testing

Integration tests use env vars: `CITECT_COMPUTER`, `CITECT_USER`, `CITECT_PASSWORD`. Defaults to `192.168.1.12` / `Manager` / `Citect`.

## quirks

- All strings encoded as GBK (Citect uses GBK, not UTF-8)
- Edition 2024 requires Rust 1.85+
- `tokio-support` feature enables async/await methods
- FFI: modify `ctapi-sys/lib/{x64,x86}/ctapi.h` + `src/lib.rs`