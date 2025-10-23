# GitHub Copilot Instructions for Oxy

## Project Context

Oxy is an open-source framework for building agentic analytics systems:

- **Backend**: Rust 2024 with Axum, SeaORM, Tokio
- **Frontend**: React 19 + TypeScript + Vite + Tailwind CSS
- **Database**: SeaORM (SQLite dev, PostgreSQL prod)
- **Monorepo**: pnpm workspaces with Turbo

## Code Style Preferences

### Rust

```rust
// Prefer Result and ? operator
fn load_config() -> Result<Config, OxyError> {
    let path = get_config_path()?;
    let content = std::fs::read_to_string(path)?;
    serde_json::from_str(&content)
        .map_err(|e| OxyError::ConfigError(e.to_string()))
}

// Use custom error types from crates/core/src/errors.rs
// Never use .unwrap() or .expect() in production code
// Always provide context in error messages
```

### TypeScript/React

```typescript
// Functional components with explicit types
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

// Never use 'any' - use proper types or 'unknown'
// Use path alias @/* for imports from src/
// Memoize expensive calculations with useMemo
// Use useCallback for callback props
```

## Naming Conventions

### Rust

- Types: `PascalCase` (e.g., `AgentExecutor`, `WorkflowEngine`)
- Functions: `snake_case` (e.g., `execute_workflow`, `load_agent`)
- Constants: `SCREAMING_SNAKE_CASE` (e.g., `DEFAULT_PORT`, `MAX_RETRIES`)
- Modules: `snake_case` (e.g., `agent_executor`, `workflow_engine`)

### TypeScript

- Components: `PascalCase` (e.g., `AgentCard`, `WorkflowEditor`)
- Functions: `camelCase` (e.g., `fetchAgentConfig`, `executeWorkflow`)
- Constants: `SCREAMING_SNAKE_CASE` (e.g., `API_BASE_URL`, `MAX_RETRIES`)
- Files: Match component/function name (e.g., `AgentCard.tsx`, `useAgentStatus.ts`)

## Common Patterns

### Error Handling

```rust
// Rust: Use ? operator and provide context
let user = User::find_by_id(id)
    .one(&db)
    .await
    .map_err(|e| OxyError::DatabaseError(e.to_string()))?
    .ok_or(OxyError::UserNotFound(id))?;
```

```typescript
// TypeScript: Use try-catch with typed errors
try {
  const result = await fetchData(id);
  return result;
} catch (error) {
  if (error instanceof APIError) {
    console.error("API failed:", error.message);
    throw error;
  }
  throw new Error("Unknown error occurred");
}
```

### Async Operations

```rust
// Rust: Use tokio for async
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

```typescript
// TypeScript: Use async/await with proper error handling
async function loadAgent(id: string): Promise<Agent> {
  const response = await fetch(`/api/agents/${id}`);
  if (!response.ok) {
    throw new APIError(`Failed to load agent: ${response.statusText}`);
  }
  return response.json();
}
```

### Database Operations

```rust
// Use SeaORM with transactions for multi-step operations
let txn = db.begin().await?;

let user = user::ActiveModel {
    email: Set(email.to_string()),
    name: Set(name.to_string()),
    ..Default::default()
}.insert(&txn).await?;

txn.commit().await?;
```

### React Hooks

```typescript
// Custom hooks for reusable logic
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

## Testing Patterns

### Rust

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

### TypeScript

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

## Quick Commands Reference

```bash
# Development
pnpm dev                          # Start dev server
cargo run -- serve                # Start API

# Testing
cargo test --workspace            # Rust tests
pnpm test                         # Frontend tests

# Linting & Formatting
cargo clippy --all-targets        # Rust lint
cargo fmt --all                   # Rust format
pnpm lint                         # TS lint
pnpm format                       # TS format

# Database
cargo run -p migration -- up      # Apply migrations
cargo run -- seed users           # Seed data
```

## Important Notes

1. **No .unwrap() in production** - Use proper error handling
2. **No 'any' type** - Use proper TypeScript types
3. **Use path alias @/** - For cleaner imports in frontend
4. **Conventional Commits** - Format: `type(scope): description`
5. **Test coverage** - Write tests for new functionality
6. **Documentation** - Add doc comments for public APIs

## Key Dependencies

### Rust

- axum (web framework)
- sea-orm (ORM)
- tokio (async runtime)
- serde (serialization)
- thiserror/anyhow (errors)

### TypeScript

- react 19
- vite 7
- tailwindcss 4
- zustand (state)
- react-query (data fetching)
- radix-ui (components)
