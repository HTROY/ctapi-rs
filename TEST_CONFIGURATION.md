# Test Configuration

The runnable SCADA examples require `CITECT_COMPUTER`, `CITECT_USER`, and `CITECT_PASSWORD`. Supply these through the test environment before launch. Use a dedicated SCADA test project and account. Some list and Tokio examples write demo tags.

The default Rust tests exercise the local wrapper without a SCADA session.

```powershell
cargo test --workspace --all-features
```

Ignored tests open an actual session. Set the same three variables for the dedicated test system before running them.

```powershell
cargo test -p ctapi-rs --all-features --lib -- --ignored
```

Use [the alarm sync guide](examples/alm-sync/README.md) for its JSON configuration and optional upload credentials.
