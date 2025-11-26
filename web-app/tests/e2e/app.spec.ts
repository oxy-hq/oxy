import { test, expect } from "@playwright/test";
import { resetProject } from "./utils";

test.describe("App Flow", () => {
  test.beforeEach(async ({ page }) => {
    resetProject();
    await page.goto("/");
  });

  test("should be able to run an app and see the result", async ({ page }) => {
    // This assumes there is at least one app available
    // Wait for the app link to be available before clicking
    // Increased timeout for CI environments where backend may take longer to initialize
    const appLink = page.locator('[data-testid^="app-link-"]').first();
    await expect(appLink).toBeVisible({ timeout: 30000 });
    await appLink.click();

    // There is no run button in the app view, it runs automatically.
    // We just need to wait for the response.

    // Wait for the AppPreview to appear
    await expect(page.getByTestId("app-preview")).toBeVisible({
      timeout: 30000,
    });

    // Verify MarkdownDisplayBlock is present
    await expect(
      page.getByTestId("app-markdown-display-block").first(),
    ).toBeVisible({ timeout: 20000 });

    // Verify DataTableBlock is present
    await expect(page.getByTestId("app-data-table-block").first()).toBeVisible({
      timeout: 20000,
    });

    // Verify LineChart is present (there may be multiple)
    await expect(page.getByTestId("app-line-chart").first()).toBeVisible({
      timeout: 20000,
    });

    // Verify BarChart is present (there may be multiple)
    await expect(page.getByTestId("app-bar-chart").first()).toBeVisible({
      timeout: 20000,
    });
  });
});
