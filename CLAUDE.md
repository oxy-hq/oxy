# Claude Code Assistant Guidelines

This file contains Claude-specific guidelines and preferences for working on this project.

> **Note**: For comprehensive development guidelines that apply to all AI assistants, see [`agents.md`](./agents.md).

## Build Guidelines

**NEVER build in release mode** - Always use debug builds:

- ✅ `cargo build -p oxy`
- ✅ `cargo check -p oxy`
- ✅ `cargo run -p oxy`
- ❌ `cargo build -p oxy --release`
- ❌ `cargo build --release`

Release builds take significantly longer and are only needed for production distributions.

**Filter build output** - Always pipe `cargo check` / `cargo build` through grep to reduce output noise:

- ✅ `cargo check -p oxy 2>&1 | grep -E "^(error|warning\[)"`
- ✅ `cargo build -p oxy 2>&1 | grep -E "^(error|warning\[)"`
- This filters out progress lines, notes, and help suggestions, keeping only actionable errors and warnings.

## Testing Guidelines

**Use cargo nextest for running tests** - Always use `cargo nextest` instead of `cargo test`:

- ✅ `cargo nextest run`
- ✅ `cargo nextest run -p oxy-app`
- ❌ `cargo test` (don't use)

Nextest provides faster, more reliable test execution with better output and parallel execution.

### Testing the CLI

After making changes to CLI commands:

```bash
# Build in debug mode
cargo build -p oxy

# Test using the debug binary
./target/debug/oxy <command>
```

### Running specific tests

```bash
# Run all tests in a package
cargo nextest run -p oxy-app

# Run a specific test file
cargo nextest run --test serve

# Run a specific test by name
cargo nextest run test_internal_port_disabled
```

## Additional Resources

- **General AI Agent Guidelines**: [`agents.md`](./agents.md) - Comprehensive development guidelines
- **GitHub Copilot Instructions**: [`.github/copilot-instructions.md`](./.github/copilot-instructions.md) - Copilot-specific patterns
