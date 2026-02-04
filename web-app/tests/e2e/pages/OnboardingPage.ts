/* eslint-disable sonarjs/no-hardcoded-passwords */
import type { Page } from "@playwright/test";

export class OnboardingPage {
  constructor(private page: Page) {}

  // Welcome Screen
  async navigateToWelcome() {
    await this.page.goto("/onboarding");
  }

  async clickGetStarted() {
    await this.page.getByRole("button", { name: /get started with github/i }).click();
  }

  // GitHub Token Page
  async enterGitHubToken(token: string) {
    await this.page.getByLabel(/github.*token/i).fill(token);
  }

  async clickValidateToken() {
    await this.page.getByRole("button", { name: /validate token/i }).click();
  }

  // Repository Selection (using combobox)
  async selectRepository(repositoryName: string) {
    // Click on the combobox to open it
    await this.page.getByRole("combobox").click();
    // Wait for the dropdown to open and select the repository
    await this.page.getByRole("option", { name: repositoryName }).click();
  }

  async clickSelectRepository() {
    await this.page.getByRole("button", { name: /^select$/i }).click();
  }

  async waitForRepositorySetup() {
    // Wait for either repository syncing or completion
    try {
      await this.page.waitForSelector('[data-testid="repository-syncing"]', {
        timeout: 5000
      });
    } catch {
      // If syncing state doesn't appear, setup might have completed quickly
      console.log("Repository setup completed quickly or syncing state not found");
    }
  }

  // Secrets Setup Page
  async enterSecret(secretName: string, value: string) {
    await this.page.getByRole("textbox", { name: secretName }).fill(value);
  }

  async clickProceedSecrets() {
    await this.page.getByRole("button", { name: /proceed/i }).click();
  }

  async fillAllRequiredSecrets() {
    // Fill in all the required secrets based on the configuration
    // Using test values for E2E testing purposes
    const testSecrets = {
      POSTGRES_PASSWORD: "test_value_postgres_123",
      GOOGLE_CLIENT_SECRET: "test_value_google_client_secret_456",
      CLICKHOUSE_PASSWORD: "test_value_clickhouse_789",
      GEMINI_API_KEY: "test_value_gemini_api_key_abc",
      AUTH_SMTP_PASSWORD: "test_value_smtp_def",
      MYSQL_PASSWORD: "test_value_mysql_ghi",
      ANTHROPIC_API_KEY: "test_value_anthropic_api_key_jkl",
      SNOWFLAKE_PASSWORD: "test_value_snowflake_mno",
      OPENAI_API_KEY: "test_value_openai_api_key_pqr"
    };

    for (const [secretName, value] of Object.entries(testSecrets)) {
      try {
        await this.enterSecret(secretName, value);
      } catch {
        console.log(`Secret ${secretName} not found or not required`);
      }
    }
  }

  async isOnboardingComplete() {
    try {
      // Check if we're on the complete step or redirected to home
      const url = this.page.url();
      if (url.includes("/onboarding/complete") || url === "/" || url.endsWith("/")) {
        return true;
      }

      // Check for completion UI elements
      const completeButton = await this.page
        .getByRole("button", { name: /continue to app/i })
        .isVisible()
        .catch(() => false);
      const successMessage = await this.page
        .getByText(/you're all set/i)
        .isVisible()
        .catch(() => false);

      return completeButton || successMessage;
    } catch {
      return false;
    }
  }

  async searchForRepository(searchTerm: string) {
    await this.page.getByTestId("repository-search").fill(searchTerm);
  }

  // Setup Complete Page
  async clickGoHome() {
    await this.page.getByRole("button", { name: /continue to app|start using oxy/i }).click();
  }

  // Utility methods
  async waitForPageLoad() {
    await this.page.waitForLoadState("networkidle");
  }

  async expectToBeOnPage(path: string) {
    await this.page.waitForURL(`**${path}**`);
  }
}
