#!/usr/bin/env bash
# .claude/hooks/session-start.sh
# Runs at the start of every Claude Code session.
# stdout is injected into Claude's context; stderr is logged silently.

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

echo "=== Session Start ==="
echo "Branch : $(git branch --show-current)"
echo "Commit : $(git log -1 --format='%h %s')"
echo ""

# Report toolchain versions — surface missing tools to Claude
echo "=== Toolchain ==="
echo "Rust   : $(rustc --version 2>/dev/null || echo 'NOT FOUND — install via: curl https://sh.rustup.rs -sSf | sh')"
echo "Cargo  : $(cargo --version 2>/dev/null || echo 'NOT FOUND')"
echo "Node   : $(node --version 2>/dev/null || echo 'NOT FOUND')"
echo "pnpm   : $(pnpm --version 2>/dev/null || echo 'NOT FOUND — install via: npm install -g pnpm')"
echo "nextest: $(cargo nextest --version 2>/dev/null || echo 'NOT FOUND — run: make install')"
echo ""

# Check for stale/missing node_modules and auto-install
if [ ! -d node_modules ] || [ package.json -nt node_modules ]; then
  echo "INFO: dependencies missing or stale — running 'make install'..."
  make install 2>&1 || echo "WARNING: 'make install' failed — check toolchain above"
else
  echo "OK: dependencies up to date"
fi

# Persist env vars for all Bash tool calls in this session
if [ -n "${CLAUDE_ENV_FILE:-}" ]; then
  echo "export OXY_DEV=true" >> "$CLAUDE_ENV_FILE"
fi

echo ""
echo "Run 'make help' to see all available targets."
