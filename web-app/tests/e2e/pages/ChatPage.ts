import { expect, type Locator, type Page } from "@playwright/test";

export class ChatPage {
  readonly page: Page;
  readonly questionInput: Locator;
  readonly agentSelectorButton: Locator;
  readonly workflowSelectorButton: Locator;
  readonly submitButton: Locator;
  readonly loadingState: Locator;
  readonly stopButton: Locator;
  readonly sendButton: Locator;
  readonly responseText: Locator;
  readonly agentMessageContainer: Locator;
  readonly agentMessageContainers: Locator;
  readonly userMessageContainer: Locator;
  readonly followUpInput: Locator;
  readonly askModeButton: Locator;
  readonly buildModeButton: Locator;
  readonly workflowModeButton: Locator;

  constructor(page: Page) {
    this.page = page;
    // The textarea's aria-label changes based on mode, so we use a flexible matcher
    this.questionInput = page.locator("textarea[name='question']");
    this.agentSelectorButton = page.getByTestId("agent-selector-button");
    this.workflowSelectorButton = page.getByTestId("workflow-selector-button");
    this.submitButton = page.getByTestId("chat-panel-submit-button");
    this.loadingState = page.getByTestId("agent-loading-state");
    this.stopButton = page.getByTestId("message-input-stop-button");
    this.sendButton = page.getByTestId("message-input-send-button");
    this.responseText = page.getByTestId("agent-response-text").last();
    this.agentMessageContainer = page.getByTestId("agent-message-container").last();
    this.agentMessageContainers = page.getByTestId("agent-message-container");
    this.userMessageContainer = page.getByTestId("user-message-container");
    this.followUpInput = page.getByRole("textbox", {
      name: "Ask a follow-up question..."
    });
    // ToggleGroup with type="single" renders as radio buttons
    this.askModeButton = page.getByRole("radio", { name: "Ask" });
    this.buildModeButton = page.getByRole("radio", { name: "Build" });
    this.workflowModeButton = page.getByRole("radio", { name: "Workflow" });
  }

  async askQuestion(
    question: string,
    agentName: string = "duckdb",
    options?: { mode?: "Ask" | "Build" | "Workflow"; workflowName?: string }
  ) {
    // Switch mode if specified
    if (options?.mode === "Build") {
      await this.buildModeButton.click();
    } else if (options?.mode === "Workflow") {
      await this.workflowModeButton.click();

      // Fill workflow title
      await this.questionInput.fill(question);

      // Select workflow
      if (options.workflowName) {
        await this.workflowSelectorButton.click();
        await this.page.getByRole("menuitemcheckbox", { name: options.workflowName }).click();
      }

      await this.submitButton.click();
      return;
    } else if (options?.mode === "Ask") {
      await this.askModeButton.click();
    }

    // Fill question
    await this.questionInput.fill(question);

    // Wait for agent selector to have loaded (button text should not be empty)
    await expect(this.agentSelectorButton).not.toHaveText("");
    await expect(this.agentSelectorButton).not.toContainText("undefined");

    // Select agent
    await this.agentSelectorButton.click();

    // Wait for dropdown menu to be visible
    await this.page.waitForTimeout(500);

    await this.page.getByRole("menuitemcheckbox", { name: agentName }).click();

    // Submit
    await this.submitButton.click();

    // Wait for navigation to thread
    await this.page.waitForURL(/\/threads\/.+/);
  }

  async askFollowUp(question: string) {
    await expect(this.followUpInput).toBeEnabled();
    await this.followUpInput.fill(question);
    await this.sendButton.click();
  }

  async waitForStreamingComplete() {
    // Wait for loading to start
    await expect(this.loadingState).toBeVisible({ timeout: 10000 });

    // Wait for loading to finish
    await this.loadingState.waitFor({
      state: "hidden",
      timeout: 60000
    });

    // Wait for streaming to stop
    await this.stopButton.waitFor({
      state: "hidden",
      timeout: 60000
    });

    // Verify send button is visible
    await expect(this.sendButton).toBeVisible({ timeout: 10000 });
  }

  async stopStreaming() {
    await this.stopButton.click();
    await expect(this.stopButton).not.toBeVisible({ timeout: 5000 });
  }

  async verifyResponse() {
    await expect(this.agentMessageContainer).toBeVisible({ timeout: 10000 });
    await expect(this.responseText).toBeVisible({ timeout: 10000 });
  }

  async verifyUserMessage(text: string) {
    const userMessage = this.page.getByTestId("user-message-text").filter({ hasText: text });
    await expect(userMessage).toBeVisible();
  }

  async getResponseCount() {
    return await this.agentMessageContainers.count();
  }

  async getUserMessageCount() {
    return await this.userMessageContainer.count();
  }

  async selectAgent(agentName: string) {
    await this.agentSelectorButton.click();
    await this.page.getByRole("menuitemcheckbox", { name: agentName }).click();
  }

  async switchMode(mode: "Ask" | "Build" | "Workflow") {
    if (mode === "Ask") {
      await this.askModeButton.click();
    } else if (mode === "Build") {
      await this.buildModeButton.click();
    } else if (mode === "Workflow") {
      await this.workflowModeButton.click();
    }
  }

  async isSubmitButtonEnabled() {
    return await this.submitButton.isEnabled();
  }
}
