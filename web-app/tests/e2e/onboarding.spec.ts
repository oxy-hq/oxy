import { test, expect } from "@playwright/test";
import { OnboardingPage } from "./pages/OnboardingPage";
import { resetProject, startServerReadonly } from "./utils";

// Declare the environment variable type
declare const process: {
  env: {
    GITHUB_PAT?: string;
  };
};

test.describe("Onboarding Flow", () => {
  let onboardingPage: OnboardingPage;
  let server_process: { kill: () => void } | null;

  test.beforeEach(async ({ page }) => {
    onboardingPage = new OnboardingPage(page);
    resetProject();
    server_process = startServerReadonly();
  });

  test.afterEach(async () => {
    if (server_process) {
      server_process.kill();
    }
  });

  test("completes the full onboarding flow successfully", async ({ page }) => {
    test.setTimeout(60 * 1000);

    // Check if GITHUB_PAT is available
    const githubPat = process.env.GITHUB_PAT;
    if (!githubPat) {
      test.skip(
        true,
        "GITHUB_PAT environment variable is required for this test",
      );
    }

    await page.goto("/");
    await page.waitForURL(/\/onboarding/);

    // Verify welcome screen content
    await expect(
      page.getByRole("heading", { name: /welcome to oxy/i }),
    ).toBeVisible();
    await expect(
      page.getByText(/connect your github repository/i),
    ).toBeVisible();
    await expect(
      page.getByRole("button", { name: /get started with github/i }),
    ).toBeVisible();

    // Step 2: Click get started and navigate to GitHub setup page
    await onboardingPage.clickGetStarted();
    await onboardingPage.expectToBeOnPage("/onboarding/setup");

    // Verify the GitHub token step is shown
    await expect(
      page.getByText("Create GitHub Personal Access Token"),
    ).toBeVisible();

    // Enter GitHub token and validate
    await onboardingPage.enterGitHubToken(githubPat!);
    await onboardingPage.clickValidateToken();

    // Wait for token validation to complete and repository step to appear
    await expect(
      page.getByRole("heading", { name: /select repository/i }),
    ).toBeVisible();

    // Select repository using the combobox
    await onboardingPage.selectRepository("haitrr/oxy-example");

    // Click the Select button to proceed with repository selection
    await onboardingPage.clickSelectRepository();

    // Wait for repository setup to complete
    await onboardingPage.waitForRepositorySetup();

    // Give some time for the onboarding state to update
    await page.waitForTimeout(3000);

    // wait for secret step
    await expect(
      page.getByRole("heading", { name: /Required Secrets Setup/i }),
    ).toBeVisible();

    // Fill in all required secrets
    await onboardingPage.fillAllRequiredSecrets();

    // Click proceed to complete secrets setup
    await onboardingPage.clickProceedSecrets();

    // Wait for secrets to be processed
    await page.waitForTimeout(2000);

    // Check if onboarding is complete
    const isComplete = await onboardingPage.isOnboardingComplete();
    expect(isComplete).toBe(true);

    // If there's a "Continue to App" button, click it
    try {
      await page
        .getByRole("button", { name: /continue to app/i })
        .click({ timeout: 2000 });
      await page.waitForURL("/", { timeout: 10000 });
    } catch {
      // If button doesn't exist or navigation happens automatically, that's ok
      console.log("No continue button found or automatic navigation occurred");
    }
  });
});
