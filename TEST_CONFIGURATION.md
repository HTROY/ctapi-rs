# Test Configuration

## Environment Variables for Tests

All integration tests that require actual Citect SCADA connection now support configuration via environment variables. This allows flexible configuration without modifying source code.

### Supported Environment Variables

The following environment variables are used for test configuration:

- `CITECT_COMPUTER`: The Citect SCADA computer/host address (default: `192.168.1.12`)
- `CITECT_USER`: The username for authentication (default: `Manager`)
- `CITECT_PASSWORD`: The password for authentication (default: `Citect`)

### Running Tests with Custom Configuration

#### Windows PowerShell

```powershell
# Set environment variables and run tests
$env:CITECT_COMPUTER = "192.168.1.100"
$env:CITECT_USER = "Admin"
$env:CITECT_PASSWORD = "MyPassword"
cargo test --lib
```

#### Windows Command Prompt

```cmd
# Set environment variables and run tests
set CITECT_COMPUTER=192.168.1.100
set CITECT_USER=Admin
set CITECT_PASSWORD=MyPassword
cargo test --lib
```

#### Linux/macOS

```bash
# Set environment variables and run tests
export CITECT_COMPUTER="192.168.1.100"
export CITECT_USER="Admin"
export CITECT_PASSWORD="MyPassword"
cargo test --lib
```

#### Single Test with Environment Variables

```bash
# Run a specific test with environment variables
CITECT_COMPUTER=192.168.1.100 CITECT_USER=Admin CITECT_PASSWORD=MyPassword \
    cargo test --lib -- --nocapture client_tag_read_ex_test
```

### Integration Tests

The following tests are available and require Citect SCADA connection:

- `client_tag_read_ex_test` - Tests tag reading with extended information
- `client_find_first_test` - Tests finding first object matching criteria
- `list_test` - Tests tag list operations
- `multi_client_test` - Tests multiple client instances
- `multi_thread_test` - Tests thread-safe client sharing
- `client_find_alarm_test` - Tests alarm data queries
- `client_drop_test` - Tests client connection cleanup

All these tests are marked with `#[ignore]` and require the `--ignored` flag to run:

```bash
cargo test --lib -- --ignored --nocapture
```

### Running Tests with Custom Configuration and Ignored Tests

```bash
# PowerShell
$env:CITECT_COMPUTER = "192.168.1.100"
cargo test --lib -- --ignored --nocapture

# Bash
CITECT_COMPUTER=192.168.1.100 cargo test --lib -- --ignored --nocapture
```

### Default Fallback Behavior

If environment variables are not set, the tests will use the following default values:

- Computer: `192.168.1.12`
- User: `Manager`
- Password: `Citect`

This ensures backward compatibility with existing test setups while allowing flexible configuration.
