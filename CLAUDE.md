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

## Testing the CLI

After making changes to CLI commands:
```bash
# Build in debug mode
cargo build -p oxy

# Test using the debug binary
./target/debug/oxy <command>
```

## Additional Resources

- **General AI Agent Guidelines**: [`agents.md`](./agents.md) - Comprehensive development guidelines
- **GitHub Copilot Instructions**: [`.github/copilot-instructions.md`](./.github/copilot-instructions.md) - Copilot-specific patterns
