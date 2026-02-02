import { spawn, execSync } from "child_process";
import { Page } from "@playwright/test";
import { writeFile, mkdir } from "fs/promises";
import { existsSync } from "fs";

const database_path = "~/.local/share/oxy";
// (Global setup handles API seeding; no base URL or project ID needed here)

/**
 * Reset the project database.
 * NOTE: This should only be called from global-setup.ts, not from individual tests.
 * Tests should run against a single persistent database setup.
 */
export function resetProject() {
  // eslint-disable-next-line sonarjs/os-command
  execSync(`rm -rf ${database_path}`);
}

export function startServer() {
  console.log("Starting server...");
  // eslint-disable-next-line sonarjs/no-os-command-from-path
  const serverProcess = spawn("cargo", ["run", "serve"], {
    stdio: "inherit",
    shell: true,
  });

  serverProcess.on("error", (err) => {
    console.error(`Failed to start server: ${err.message}`);
  });

  console.log("Server started successfully.");
  return serverProcess;
}

// Reset the dedicated test file to its original content
export async function resetTestFile() {
  // File should be in the examples directory of the oxy-internal project
  const testFilePath = "../examples/test-file-for-e2e.txt";
  const originalContent = `# Test File for E2E Tests

This file is used for IDE E2E tests.
It gets modified during tests and reset after each test.
`;

  // Ensure examples directory exists
  if (!existsSync("../examples")) {
    await mkdir("../examples", { recursive: true });
  }

  await writeFile(testFilePath, originalContent, "utf-8");
}

// Reset the dedicated test agent file to its original content
export async function resetTestAgentFile() {
  const testAgentPath = "../examples/agents/test-agent-e2e.agent.yml";
  const originalContent = `# Test Agent for E2E Tests
description: "A test agent used for IDE E2E tests"
name: test-agent-e2e

model: "openai-4o-mini"

system_instructions: |
  You are a test agent used for E2E testing.
  This file gets modified during tests and reset after each test.

output_format: default

tools:
  - name: execute_sql
    type: execute_sql
    database: local
`;

  // Ensure agents directory exists
  if (!existsSync("../examples/agents")) {
    await mkdir("../examples/agents", { recursive: true });
  }

  await writeFile(testAgentPath, originalContent, "utf-8");
}

// Delete test files created during tests
export async function cleanupTestFiles() {
  const { unlink, readdir } = await import("fs/promises");
  const { existsSync } = await import("fs");
  const path = await import("path");

  const examplesDir = "../examples";

  if (!existsSync(examplesDir)) {
    return;
  }

  try {
    // Patterns to match test files
    const testPatterns = [
      /^test-create-\d+\.txt$/,
      /^nested-test-\d+\.txt$/,
      /^test-folder-\d+$/,
      /^test-escape-file\.txt$/,
      /^test-spaces\.txt$/,
      /^a{1000,}\.txt$/,
      /^test-error-file\.txt$/,
      /^test-network-file\.txt$/,
      /^test-renamed-\d+\.txt$/,
      /^renamed-.*\.txt$/,
    ];

    // Delete files in root examples directory
    const files = await readdir(examplesDir);
    for (const file of files) {
      if (testPatterns.some(pattern => pattern.test(file))) {
        const filePath = path.join(examplesDir, file);
        await unlink(filePath).catch(() => {});
        console.log(`Cleaned up: ${file}`);
      }
    }

    // Delete test files in workflows subdirectory
    const workflowsDir = path.join(examplesDir, "workflows");
    if (existsSync(workflowsDir)) {
      const workflowFiles = await readdir(workflowsDir);
      for (const file of workflowFiles) {
        if (testPatterns.some(pattern => pattern.test(file))) {
          const filePath = path.join(workflowsDir, file);
          await unlink(filePath).catch(() => {});
          console.log(`Cleaned up: workflows/${file}`);
        }
      }
    }
  } catch (error) {
    console.error("Error cleaning up test files:", error);
  }
}

// (Removed seedThreadsDataViaAPI to centralize seeding in global setup only)

// Create test threads via UI - slower but more realistic
export async function seedThreadsData(page: Page, count: number = 10) {
  for (let i = 0; i < count; i++) {
    await page.goto("/");

    // Fill and submit in one flow without waiting for animations
    const questionInput = page.getByRole("textbox", { name: "Ask anything" });
    await questionInput.fill(`Test thread ${i + 1}`);

    // Click agent selector and select duckdb
    await page.getByTestId("agent-selector-button").click();
    await page.getByRole("menuitemcheckbox", { name: "duckdb" }).click();

    // Submit and immediately move to next (don't wait for response)
    await page.getByTestId("chat-panel-submit-button").click();

    // Just wait for URL change, don't wait for loading/animations
    await page.waitForURL(/\/threads\/.+/, { timeout: 5000 });
  }
}
