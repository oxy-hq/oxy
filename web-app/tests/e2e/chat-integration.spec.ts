import { test, expect } from "@playwright/test";
import { resetProject } from "./utils";
import { ChatPage } from "./pages/ChatPage";

test.describe("Home Page Chat Box Test", () => {
  test.beforeEach(async ({ page }) => {
    resetProject();
    await page.goto("/");
  });

  test("should be able to ask a question and get a response", async ({
    page,
  }) => {
    const chatPage = new ChatPage(page);

    // Ask question
    await chatPage.askQuestion("Top 3 fruit sales?", "duckdb");

    // Wait for response
    await chatPage.waitForStreamingComplete();

    // Verify response
    await chatPage.verifyResponse();

    // Verify artifacts (SQL queries) are visible
    await expect(page.getByTestId("agent-artifact").first()).toBeVisible({
      timeout: 10000,
    });

    const artifact = page.getByTestId("agent-artifact").first();
    await expect(artifact).toHaveAttribute("data-artifact-kind", "execute_sql");

    // Verify follow-up input is enabled
    await expect(chatPage.followUpInput).toBeEnabled();
  });

  test("should be able to cancel streaming with stop button", async ({
    page,
  }) => {
    const chatPage = new ChatPage(page);

    // Ask question
    await chatPage.askQuestion("Top 3 fruit sales?", "duckdb");

    // Wait for streaming to start
    await expect(chatPage.stopButton).toBeVisible({ timeout: 10000 });

    // Cancel streaming
    await chatPage.stopStreaming();

    // Verify cancellation message
    await expect(page.getByText("🔴 Operation cancelled")).toBeVisible({
      timeout: 10000,
    });

    // Verify follow-up input is enabled
    await expect(chatPage.followUpInput).toBeEnabled();

    // Verify some response was shown
    await expect(chatPage.agentMessageContainer).toBeVisible();
  });

  test("should be able to run a workflow from chat box", async ({ page }) => {
    const chatPage = new ChatPage(page);

    // Run workflow
    await chatPage.askQuestion("run this workflow", "duckdb", {
      mode: "Workflow",
      workflowName: "fruit_sales_report",
    });

    // Wait for workflow completion
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

  test("should be able to ask a follow-up question in thread", async ({
    page,
  }) => {
    const chatPage = new ChatPage(page);

    // Ask initial question
    await chatPage.askQuestion("Top 3 fruit sales?", "duckdb");
    await chatPage.waitForStreamingComplete();

    // Ask follow-up
    await chatPage.askFollowUp("What about the bottom 3?");
    await chatPage.waitForStreamingComplete();

    // Verify we have 2 agent responses
    const responseCount = await chatPage.getResponseCount();
    expect(responseCount).toBe(2);
  });

  test("should be able to select different agents", async ({ page }) => {
    const chatPage = new ChatPage(page);

    // Click on agent selector
    await chatPage.agentSelectorButton.click();

    // Verify multiple agents are available
    await expect(
      page.getByRole("menuitemcheckbox", { name: "duckdb" }),
    ).toBeVisible();
    await expect(
      page.getByRole("menuitemcheckbox", { name: "_routing" }),
    ).toBeVisible();

    // Select semantic agent if available
    const semanticAgent = page.getByRole("menuitemcheckbox", {
      name: "semantic",
      exact: true,
    });
    if (await semanticAgent.isVisible()) {
      await semanticAgent.click();

      // Close dropdown
      await page.keyboard.press("Escape");

      // Verify selected agent
      await expect(chatPage.agentSelectorButton).toContainText("semantic");
    }
  });

  test("should show submit button disabled when input is empty", async ({
    page,
  }) => {
    const chatPage = new ChatPage(page);

    // Verify submit button is disabled initially
    await expect(chatPage.submitButton).toBeDisabled();

    // Type something
    await chatPage.questionInput.fill("test");

    // Submit button should be enabled
    await expect(chatPage.submitButton).toBeEnabled();

    // Clear the input
    await chatPage.questionInput.clear();

    // Submit button should be disabled again
    await expect(chatPage.submitButton).toBeDisabled();
  });

  test("should display user message in thread", async ({ page }) => {
    const chatPage = new ChatPage(page);

    const userQuestion = "What are the top selling fruits?";

    // Ask question
    await chatPage.askQuestion(userQuestion, "duckdb");

    // Verify user message is displayed
    await chatPage.verifyUserMessage(userQuestion);
  });

  test("should switch between Ask, Build, and Workflow modes", async ({
    page,
  }) => {
    const chatPage = new ChatPage(page);

    // Verify Ask is selected by default
    await expect(chatPage.askModeRadio).toBeChecked();

    // Switch to Build mode
    await chatPage.switchMode("Build");
    await expect(chatPage.buildModeRadio).toBeChecked();
    await expect(
      page.getByRole("textbox", { name: "Enter anything you want to build" }),
    ).toBeVisible();

    // Switch to Workflow mode
    await chatPage.switchMode("Workflow");
    await expect(chatPage.workflowModeRadio).toBeChecked();
    await expect(
      page.getByRole("textbox", { name: "Enter a title for this" }),
    ).toBeVisible();
    await expect(chatPage.workflowSelectorButton).toBeVisible();

    // Switch back to Ask
    await chatPage.switchMode("Ask");
    await expect(chatPage.askModeRadio).toBeChecked();
    await expect(chatPage.questionInput).toBeVisible();
  });
});
