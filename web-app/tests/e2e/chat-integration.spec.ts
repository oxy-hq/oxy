import { expect, test } from "@playwright/test";
import { ChatPage } from "./pages/ChatPage";

test.describe("Home Page Chat Box Test", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    // Wait for network to be idle to ensure backend API calls have completed
    await page.waitForLoadState("networkidle");

    // Wait for ChatPanel to be visible
    await expect(page.locator("textarea[name='question']")).toBeVisible({
      timeout: 10000
    });
  });

  test("should be able to ask a question and get a response", async ({ page }) => {
    const chatPage = new ChatPage(page);

    await chatPage.askQuestion("Top 3 fruit sales?", "duckdb");

    // Wait for response
    await chatPage.waitForStreamingComplete();

    await chatPage.verifyResponse();

    // Verify artifacts (SQL queries) are visible
    await expect(page.getByTestId("agent-artifact").first()).toBeVisible({
      timeout: 10000
    });

    const artifact = page.getByTestId("agent-artifact").first();
    await expect(artifact).toHaveAttribute("data-artifact-kind", "execute_sql");

    await expect(chatPage.followUpInput).toBeEnabled();
  });

  test("should be able to cancel streaming with stop button", async ({ page }) => {
    const chatPage = new ChatPage(page);

    await chatPage.askQuestion("Top 3 fruit sales?", "duckdb");

    // Wait for streaming to start
    await expect(chatPage.stopButton).toBeVisible({ timeout: 10000 });

    await chatPage.stopStreaming();

    // Verify cancellation message
    await expect(page.getByText("🔴 Operation cancelled")).toBeVisible({
      timeout: 10000
    });

    await expect(chatPage.followUpInput).toBeEnabled();

    // Verify some response was shown
    await expect(chatPage.agentMessageContainer).toBeVisible();
  });

  test("should be able to run a workflow from chat box", async ({ page }) => {
    const chatPage = new ChatPage(page);

    // Run automation
    await chatPage.askQuestion("run this workflow", "duckdb", {
      mode: "Workflow",
      automationName: "fruit_sales_report"
    });

    // Wait for automation completion
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
      { timeout: 60000 }
    );

    await expect(page.getByText("⏳Starting query_data").first()).toBeVisible({
      timeout: 10000
    });

    await expect(page.getByText("✅Workflow executed successfully").first()).toBeVisible({
      timeout: 30000
    });
  });

  test("should be able to ask a follow-up question in thread", async ({ page }) => {
    const chatPage = new ChatPage(page);

    await chatPage.askQuestion("Top 3 fruit sales?", "duckdb");
    await chatPage.waitForStreamingComplete();

    await chatPage.askFollowUp("What about the bottom 3?");
    await chatPage.waitForStreamingComplete();

    // Verify we have 2 agent responses
    const responseCount = await chatPage.getResponseCount();
    expect(responseCount).toBe(2);
  });

  test("should be able to select different agents", async ({ page }) => {
    const chatPage = new ChatPage(page);

    // Wait for agent selector to have loaded
    await expect(chatPage.agentSelectorButton).not.toHaveText("");
    await expect(chatPage.agentSelectorButton).not.toContainText("undefined");

    await chatPage.agentSelectorButton.click();

    // Wait for dropdown to open
    await page.waitForTimeout(500);

    // Verify multiple agents are available (update with actual agent names in your environment)
    await expect(page.getByRole("menuitemcheckbox", { name: "duckdb" })).toBeVisible();
    await expect(page.getByRole("menuitemcheckbox", { name: "_routing" })).toBeVisible();

    const semanticAgent = page.getByRole("menuitemcheckbox", {
      name: "semantic",
      exact: true
    });
    if (await semanticAgent.isVisible()) {
      await semanticAgent.click();

      // Close dropdown
      await page.keyboard.press("Escape");

      // Verify selected agent
      await expect(chatPage.agentSelectorButton).toContainText("semantic");
    }
  });

  test("should show submit button disabled when input is empty", async ({ page }) => {
    const chatPage = new ChatPage(page);

    // Verify submit button is disabled initially
    await expect(chatPage.submitButton).toBeDisabled();

    // Type something
    await chatPage.questionInput.fill("test");

    await expect(chatPage.submitButton).toBeEnabled();

    await chatPage.questionInput.clear();

    await expect(chatPage.submitButton).toBeDisabled();
  });

  test("should display user message in thread", async ({ page }) => {
    const chatPage = new ChatPage(page);

    const userQuestion = "What are the top selling fruits?";

    await chatPage.askQuestion(userQuestion, "duckdb");

    await chatPage.verifyUserMessage(userQuestion);
  });

  test("should switch between Ask, Build, and Workflow modes", async ({ page }) => {
    const chatPage = new ChatPage(page);

    // Wait for mode buttons to be visible
    await expect(chatPage.askModeButton).toBeVisible({ timeout: 10000 });

    // Verify Ask is selected by default (radio button is checked)
    await expect(chatPage.askModeButton).toBeChecked();

    await chatPage.switchMode("Build");
    await expect(chatPage.buildModeButton).toBeChecked();
    await expect(chatPage.questionInput).toHaveAttribute(
      "placeholder",
      "Enter anything you want to build"
    );

    await chatPage.switchMode("Workflow");
    await expect(chatPage.automationModeButton).toBeChecked();
    await expect(chatPage.questionInput).toHaveAttribute(
      "placeholder",
      "Enter a title for this workflow run"
    );
    await expect(chatPage.automationSelectorButton).toBeVisible();

    await chatPage.switchMode("Ask");
    await expect(chatPage.askModeButton).toBeChecked();
    await expect(chatPage.questionInput).toHaveAttribute("placeholder", "Ask anything");
  });
});
