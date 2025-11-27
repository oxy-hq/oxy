import { spawn, execSync } from "child_process";
import { Page } from "@playwright/test";
import { writeFile } from "fs/promises";
const database_path = "~/.local/share/oxy";

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
  const testFilePath = "examples/test-file-for-e2e.txt";
  const originalContent = `# Test File for E2E Tests

This file is used for IDE E2E tests.
It gets modified during tests and reset after each test.
`;
  await writeFile(testFilePath, originalContent, "utf-8");
}

// Seed dummy threads data for testing
export async function seedThreadsData(page: Page, count: number = 15) {
  // Create dummy threads by making API calls or interacting with the UI
  for (let i = 0; i < count; i++) {
    await page.goto("/");

    // Fill in question
    const questionInput = page.getByRole("textbox", { name: "Ask anything" });
    await questionInput.fill(`Test thread ${i + 1}`);

    // Select duckdb agent
    await page.getByTestId("agent-selector-button").click();
    await page.getByRole("menuitemcheckbox", { name: "duckdb" }).click();

    // Submit without waiting for response
    await page.getByTestId("chat-panel-submit-button").click();

    // Wait for URL to change to thread page
    await page.waitForURL(/\/threads\/.+/, { timeout: 5000 });

    // Small delay between threads
    await page.waitForTimeout(100);
  }
}
