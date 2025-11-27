# E2E Testing Guide

## Quick Start

```bash
# start the backend manually

docker-compose up -d # start postgres
cd examples
export OXY_DATABASE_URL=postgresql://admin:password@localhost:5432/default
cargo run -- serve --http2-only

# in another terminal, start the end to end tests
cd web-app
pnpm test:e2e                    # Run all tests (headless, won't steal focus)
pnpm test:e2e chat-integration   # Run specific file
HEADED=1 pnpm test:e2e           # See browser (headed mode)
pnpm test:e2e --debug            # Debug mode with Playwright Inspector
```

**Note**: Tests run headless by default so you can continue using your desktop. Use `HEADED=1` to see the browser.

## Architecture

Tests use **Page Object Pattern** for maintainability. Each page/feature has a class encapsulating selectors and actions.

```typescript
// Example usage
const chatPage = new ChatPage(page);
await chatPage.askQuestion("What are top sales?", "duckdb");
await chatPage.waitForResponse();
```

## Page Objects

Located in `pages/` directory:

- `ChatPage` - Chat interactions, agent selection, messaging
- `IDEPage` - File browsing, editing, folder navigation
- `ThreadsPage` - Thread listing, pagination, selection
- `NavigationPage` - Sidebar, page routing
- `OntologyPage` - Ontology graph interactions

## Test Structure

```typescript
import { test, expect } from "@playwright/test";
import { resetProject } from "./utils";
import { ChatPage } from "./pages/ChatPage";

test.describe("Feature", () => {
  test.beforeEach(async ({ page }) => {
    resetProject();
    await page.goto("/");
  });

  test("should do something", async ({ page }) => {
    const chatPage = new ChatPage(page);
    await chatPage.askQuestion("question");
    await expect(chatPage.responseText).toBeVisible();
  });
});
```

## Coverage

| Feature         | Status | File                     |
| --------------- | ------ | ------------------------ |
| Chat Q&A        | ✅     | chat-integration.spec.ts |
| Follow-ups      | ✅     | chat-integration.spec.ts |
| Agent selection | ✅     | chat-integration.spec.ts |
| IDE editing     | ✅     | ide.spec.ts              |
| File navigation | ✅     | ide.spec.ts              |
| Thread listing  | ✅     | threads-listing.spec.ts  |
| Pagination      | ✅     | threads-listing.spec.ts  |
| Navigation      | ✅     | navigation.spec.ts       |
| Workflows       | ✅     | workflow.spec.ts         |
| Apps            | ✅     | app.spec.ts              |

## Best Practices

**✅ Do:**

- Use Page Objects for all interactions
- Wait for loading states: `await page.waitForLoadingToComplete()`
- Use test IDs for reliable selectors
- Keep tests independent with `resetProject()`
- Test user flows, not implementation

**❌ Don't:**

- Use `page.waitForTimeout()` - use explicit waits
- Share state between tests
- Use CSS selectors when test IDs exist
- Leave `.only()` or `.skip()` in commits

## Common Patterns

### Wait for Streaming

```typescript
await chatPage.waitForStreamingComplete();
```

### Navigate and Verify

```typescript
const nav = new NavigationPage(page);
await nav.goToThreads();
await expect(page).toHaveURL(/\/threads/);
```

### Edit File in IDE

```typescript
const idePage = new IDEPage(page);
await idePage.openFile("README.md");
await idePage.editFile("New content");
await idePage.saveFile();
```

## Debugging

```bash
HEADED=1 pnpm test:e2e          # Watch test run in browser
pnpm test:e2e --debug           # Playwright Inspector
pnpm test:e2e --ui              # Playwright UI mode (interactive)
```

**Troubleshooting:**

- Screenshots/videos saved to `test-results/` on failure
- Use `--ui` mode for interactive debugging with timeline
- Tests persist browser state (localStorage) - use `resetProject()` in beforeEach

## Resources

- [Playwright Docs](https://playwright.dev)
- Page Objects: `/tests/e2e/pages/`
- Test Utils: `/tests/e2e/utils.ts`
