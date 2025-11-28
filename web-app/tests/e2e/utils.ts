import { spawn, execSync } from "child_process";
import { Page, request } from "@playwright/test";
import { writeFile } from "fs/promises";
import { mkdirSync, existsSync } from "fs";

const database_path = "~/.local/share/oxy";
const API_BASE_URL = "https://localhost:3000/api";
const PROJECT_ID = "default";

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
  // Ensure the examples directory exists
  if (!existsSync("examples")) {
    mkdirSync("examples", { recursive: true });
  }
  await writeFile(testFilePath, originalContent, "utf-8");
}

// Create test threads via API - much faster than UI
export async function seedThreadsDataViaAPI(count: number = 15) {
  const apiContext = await request.newContext({
    ignoreHTTPSErrors: true,
  });

  const promises = [];
  for (let i = 0; i < count; i++) {
    const promise = apiContext.post(`${API_BASE_URL}/${PROJECT_ID}/threads`, {
      data: {
        title: `Test thread ${i + 1}`,
        input: `Test thread ${i + 1}`,
        source: "duckdb",
        source_type: "agent",
      },
    });
    promises.push(promise);
  }

  // Wait for all threads to be created in parallel
  await Promise.all(promises);
  await apiContext.dispose();
}

// Create test threads via UI - slower but more realistic
export async function seedThreadsData(page: Page, count: number = 15) {
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
