import { test, expect } from "@playwright/test";
import { resetProject } from "./utils";

test.describe("Home Page Chat Box Test", () => {
  test.beforeEach(async ({ page }) => {
    resetProject();
    await page.goto("/");
  });

  test("should be able to ask a question and get a response", async ({
    page,
  }) => {
    // Fill in the chat input
    await page
      .getByRole("textbox", { name: "Ask anything" })
      .fill("Top 3 fruit sales?");

    // Select the duckdb agent
    await page.getByTestId("agent-selector-button").click();
    await page.getByRole("menuitemcheckbox", { name: "duckdb" }).click();

    // Submit the question
    await page.getByTestId("chat-panel-submit-button").click();

    // Wait for navigation to thread page
    await page.waitForURL(/\/threads\/.+/);

    // Wait for the loading state to appear
    await expect(page.getByTestId("agent-loading-state")).toBeVisible({
      timeout: 10000,
    });

    // Wait for the agent's response to start appearing (loading state disappears)
    await page
      .getByTestId("agent-loading-state")
      .waitFor({ state: "hidden", timeout: 60000 });

    // Wait for streaming to complete - stop button (X) disappears and send button (arrow) appears
    await page
      .getByTestId("message-input-stop-button")
      .waitFor({ state: "hidden", timeout: 60000 });
    await expect(page.getByTestId("message-input-send-button")).toBeVisible({
      timeout: 10000,
    });

    // Verify the agent message container is visible
    await expect(page.getByTestId("agent-message-container")).toBeVisible({
      timeout: 10000,
    });

    // Verify text content is present
    await expect(page.getByTestId("agent-response-text")).toBeVisible({
      timeout: 10000,
    });

    // Verify artifacts (SQL queries) are visible
    await expect(page.getByTestId("agent-artifact").first()).toBeVisible({
      timeout: 10000,
    });

    // Verify the artifact is of type execute_sql
    const artifact = page.getByTestId("agent-artifact").first();
    await expect(artifact).toHaveAttribute("data-artifact-kind", "execute_sql");

    // Verify the follow-up input is now enabled
    await expect(
      page.getByRole("textbox", { name: "Ask a follow-up question..." }),
    ).toBeEnabled();
  });

  test("should be able to cancel streaming with stop button", async ({
    page,
  }) => {
    // Fill in the chat input
    await page.getByRole("textbox", { name: "Ask anything" }).click();
    await page
      .getByRole("textbox", { name: "Ask anything" })
      .fill("Top 3 fruit sales?");

    // Select the duckdb agent
    await page.getByTestId("agent-selector-button").click();
    await page.getByRole("menuitemcheckbox", { name: "duckdb" }).click();

    // Submit the question
    await page.getByTestId("chat-panel-submit-button").click();

    // Wait for navigation to thread page
    await page.waitForURL(/\/threads\/.+/);

    // Wait for the loading state to appear
    await expect(page.getByTestId("agent-loading-state")).toBeVisible({
      timeout: 10000,
    });

    // Wait for the agent's response to start appearing (loading state disappears)
    await page
      .getByTestId("agent-loading-state")
      .waitFor({ state: "hidden", timeout: 60000 });

    // Wait for stop button to appear (streaming has started)
    await expect(page.getByTestId("message-input-stop-button")).toBeVisible({
      timeout: 10000,
    });

    // Click the stop button to cancel streaming
    await page.getByTestId("message-input-stop-button").click();

    // Verify stop button disappears and send button appears (streaming cancelled)
    await page
      .getByTestId("message-input-stop-button")
      .waitFor({ state: "hidden", timeout: 30000 });
    await expect(page.getByTestId("message-input-send-button")).toBeVisible({
      timeout: 10000,
    });

    // Verify the cancellation message appears
    await expect(page.getByText("🔴 Operation cancelled")).toBeVisible({
      timeout: 10000,
    });

    // Verify the follow-up input is enabled after cancellation
    await expect(
      page.getByRole("textbox", { name: "Ask a follow-up question..." }),
    ).toBeEnabled();

    // Verify that some response was shown before cancellation
    await expect(page.getByTestId("agent-message-container")).toBeVisible();
  });

  test("should be able to run a workflow from chat box", async ({ page }) => {
    // Select the Workflow primitive button
    await page.getByRole("radio", { name: "Workflow" }).click();

    // Fill in the workflow title
    await page
      .getByRole("textbox", { name: "Enter a title for this" })
      .fill("run this workflow");

    // Select the fruit_sales_report workflow from the dropdown
    await page.getByTestId("workflow-selector-button").click();
    await page
      .getByRole("menuitemcheckbox", { name: "fruit_sales_report" })
      .click();

    // Submit the workflow
    await page.getByTestId("chat-panel-submit-button").click();

    // Wait for navigation to the thread page
    await page.waitForURL(/\/threads\/.+/);

    // Wait for the workflow to complete by monitoring the thread workflow API
    await page.waitForResponse(
      (response) => {
        const url = response.url();
        return (
          url.includes("/api/") &&
          url.includes("/threads/") &&
          url.includes("/workflow") &&
          response.status() === 200
        );
      },
      { timeout: 60000 },
    );

    await expect(page.getByText("⏳Starting query_data").first()).toBeVisible({
      timeout: 10000,
    });

    await expect(
      page.getByText("✅Workflow executed successfully").first(),
    ).toBeVisible({
      timeout: 30000,
    });
  });
});
