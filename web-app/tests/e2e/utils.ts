import { spawn, execSync } from "child_process";
import { Page } from "@playwright/test";
import { writeFile, mkdir } from "fs/promises";
import { existsSync } from "fs";

const database_path = "~/.local/share/oxy";
// (Global setup handles API seeding; no base URL or project ID needed here)

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
