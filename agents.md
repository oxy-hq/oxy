# AI Agent Guidelines for Oxy Development

This document provides guidelines for AI coding agents (like Claude, GitHub Copilot, Cursor, etc.) working on the Oxy project.

## Project Overview

Oxy is an open-source framework for building agentic analytics systems:
- **Core Language**: Rust 2024 (edition 2024, rust-version 1.92.0)
- **Frontend**: React 19 + TypeScript + Vite + Tailwind CSS
- **Database**: SeaORM (SQLite dev, PostgreSQL prod)
- **Monorepo**: pnpm workspaces with Turbo

## Quick Reference

### Build Commands
```bash
# Rust (ALWAYS use debug mode unless explicitly told otherwise)
cargo build -p oxy          # Build CLI
cargo test --workspace      # Run tests
cargo clippy --all-targets  # Lint
cargo fmt --all             # Format

# Frontend
cd web-app
pnpm dev                    # Dev server
pnpm build                  # Production build
pnpm lint                   # ESLint
pnpm format                 # Prettier

# Database
cargo run -p migration -- up    # Apply migrations
cargo run -- seed users         # Seed data
```

### Important Files
- **Rust Entry Point**: `crates/core/src/lib.rs`
- **CLI**: `crates/core/src/main.rs`
- **Frontend Entry**: `web-app/src/main.tsx`
- **API Routes**: `crates/core/src/api/router.rs`
- **Database Entities**: `crates/entity/src/`

## Code Style Guidelines

### Rust Best Practices

1. **Error Handling**
   ```rust
   // ✅ Good: Use Result and ? operator
   fn load_config() -> Result<Config, OxyError> {
       let path = get_config_path()?;
       let content = std::fs::read_to_string(path)?;
       serde_json::from_str(&content)
           .map_err(|e| OxyError::ConfigurationError(e.to_string()))
   }
   
   // ❌ Bad: Never use unwrap/expect in production
   let config = serde_json::from_str(&content).unwrap();
   ```

2. **Naming Conventions**
   - Types: `PascalCase` → `AgentExecutor`, `WorkflowEngine`
   - Functions: `snake_case` → `execute_workflow`, `load_agent`
   - Constants: `SCREAMING_SNAKE_CASE` → `DEFAULT_PORT`, `MAX_RETRIES`

3. **Async/Await**
   ```rust
   // Use tokio for async operations
   #[tokio::main]
   async fn main() -> Result<(), OxyError> {
       let result = execute_workflow("workflow.yaml").await?;
       Ok(())
   }
   
   // For CPU-intensive work, use spawn_blocking
   tokio::task::spawn_blocking(|| {
       expensive_computation()
   }).await??;
   ```

### TypeScript/React Best Practices

1. **Type Safety**
   ```typescript
   // ✅ Good: Explicit types
   interface ButtonProps {
     label: string;
     onClick: () => void;
     variant?: 'primary' | 'secondary';
   }
   
   export function Button({ label, onClick, variant = 'primary' }: ButtonProps) {
     return (
       <button onClick={onClick} className={`btn-${variant}`}>
         {label}
       </button>
     );
   }
   
   // ❌ Bad: Never use 'any'
   function process(data: any) { ... }
   
   // ✅ Good: Use unknown if type is truly unknown
   function process(data: unknown) { ... }
   ```

2. **Naming Conventions**
   - Components: `PascalCase` → `AgentCard`, `WorkflowEditor`
   - Functions: `camelCase` → `fetchAgentConfig`, `executeWorkflow`
   - Files: Match component name → `AgentCard.tsx`, `useAgentStatus.ts`

3. **React Patterns**
   - Use functional components with hooks
   - Memoize expensive calculations with `useMemo`
   - Use `useCallback` for callback props
   - Extract reusable logic into custom hooks

## Common Patterns

### Database Operations (SeaORM)
```rust
use sea_orm::*;

// Use transactions for multi-step operations
let txn = db.begin().await?;

let user = user::ActiveModel {
    email: Set(email.to_string()),
    name: Set(name.to_string()),
    ..Default::default()
}.insert(&txn).await?;

txn.commit().await?;
```

### API Error Handling (TypeScript)
```typescript
try {
  const response = await fetch(`/api/agents/${id}`);
  if (!response.ok) {
    throw new APIError(`Failed to load agent: ${response.statusText}`);
  }
  return response.json();
} catch (error) {
  if (error instanceof APIError) {
    console.error("API failed:", error.message);
    throw error;
  }
  throw new Error("Unknown error occurred");
}
```

### Custom React Hooks
```typescript
function useAgentStatus(agentId: string) {
  const [status, setStatus] = useState<AgentStatus | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;

    async function fetch() {
      const result = await getAgentStatus(agentId);
      if (!cancelled) setStatus(result);
      setLoading(false);
    }

    fetch();
    return () => {
      cancelled = true;
    };
  }, [agentId]);

  return { status, loading };
}
```

## Testing Guidelines

### Rust Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_parsing() {
        let yaml = r#"name: test"#;
        let workflow = parse_workflow(yaml).unwrap();
        assert_eq!(workflow.name, "test");
    }

    #[tokio::test]
    async fn test_async_operation() {
        let result = async_function().await;
        assert!(result.is_ok());
    }
}
```

### TypeScript Tests (Vitest)
```typescript
import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';

describe('AgentCard', () => {
  it('renders agent name', () => {
    render(<AgentCard name="Test Agent" agentId="123" onExecute={() => {}} />);
    expect(screen.getByText('Test Agent')).toBeInTheDocument();
  });
});
```

## AI Assistant Behavior

### When Writing Code
1. ✅ **Understand First**: Ask clarifying questions if requirements are unclear
2. ✅ **Follow Patterns**: Match existing code style and patterns in the codebase
3. ✅ **Test Coverage**: Include tests for new functionality
4. ✅ **Documentation**: Add doc comments for public APIs
5. ✅ **Error Handling**: Always handle errors appropriately
6. ✅ **No Unwrap**: Never use `.unwrap()` or `.expect()` in production code

### When Refactoring
1. Explain the reasoning behind changes
2. Ensure backward compatibility unless explicitly breaking
3. Update related tests and documentation
4. Run full test suite before proposing changes

### When Debugging
1. Reproduce the issue first
2. Check logs and error messages
3. Verify assumptions with tests
4. Explain the root cause and fix

## Git Conventions

Use [Conventional Commits](https://www.conventionalcommits.org/):
- Format: `<type>(<scope>): <description>`
- Types: `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `chore`
- Examples:
  - `feat(core): add semantic query optimization`
  - `fix(migration): resolve connection pool leak`
  - `docs(readme): update installation instructions`

## Security Guidelines

1. **Secrets Management**
   - Never commit secrets or API keys
   - Use `.env` files (not tracked in git)
   - Use environment variables for configuration

2. **Input Validation**
   - Validate all external inputs
   - Sanitize user-provided data
   - Use parameterized queries for database operations

3. **Dependencies**
   - Keep dependencies up to date
   - Review security advisories: `cargo audit`
   - Minimize dependency footprint

## Performance Considerations

### Rust
- Use `release` profile for benchmarking only (not for development)
- Profile with `cargo flamegraph` or `perf`
- Consider using `Arc` and `Mutex` for shared state
- Prefer `&str` over `String` for read-only strings

### Frontend
- Code splitting for large bundles
- Lazy load components when appropriate
- Optimize re-renders with `memo` and `useMemo`
- Use production builds for performance testing

## Documentation Standards

### Rust Doc Comments
```rust
/// Processes analytics data for the given agent.
///
/// # Arguments
/// * `agent_id` - The unique identifier of the agent
/// * `data` - The raw analytics data to process
///
/// # Returns
/// Returns `Ok(ProcessedData)` on success, or an `OxyError` on failure.
///
/// # Errors
/// * `OxyError::InvalidAgent` - If the agent_id is not found
/// * `OxyError::ProcessingError` - If data processing fails
///
/// # Example
/// ```
/// let result = process_analytics("agent-123", raw_data)?;
/// ```
pub fn process_analytics(agent_id: &str, data: RawData) -> Result<ProcessedData, OxyError> {
    // implementation
}
```

### TypeScript JSDoc
```typescript
/**
 * Fetches agent configuration from the API.
 *
 * @param agentId - The unique identifier of the agent
 * @returns Promise resolving to the agent configuration
 * @throws {APIError} If the API request fails
 *
 * @example
 * ```typescript
 * const config = await fetchAgentConfig('agent-123');
 * ```
 */
export async function fetchAgentConfig(agentId: string): Promise<AgentConfig> {
  // implementation
}
```

## Resources

- **Documentation**: https://oxy.tech/docs
- **DeepWiki**: https://deepwiki.com/oxy-hq/oxy
- **Repository**: https://github.com/oxy-hq/oxy
- **Issues**: https://github.com/oxy-hq/oxy/issues
- **GitHub Copilot Instructions**: `.github/copilot-instructions.md`
- **Claude-specific Guidelines**: `CLAUDE.md`

---

**Remember**: Write clear, maintainable, well-tested code that follows Rust and TypeScript best practices. When in doubt, favor simplicity and clarity over cleverness.
