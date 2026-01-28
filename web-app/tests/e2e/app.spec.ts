import { test, expect } from "@playwright/test";

test.describe("App Flow", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    // Wait for network to be idle to ensure backend API calls have completed
    await page.waitForLoadState("networkidle");
  });

  test("should be able to run an app and see the result", async ({ page }) => {
    // Navigate to the "example" app
    const appLink = page.getByTestId("app-link-example");
    await expect(appLink).toBeVisible({ timeout: 15000 });
    await appLink.click();

    // There is no run button in the app view, it runs automatically.
    // We just need to wait for the response.

    // Wait for the AppPreview to appear
    await expect(page.getByTestId("app-preview")).toBeVisible({
      timeout: 15000,
    });

    // Verify MarkdownDisplayBlock is present
    await expect(
      page.getByTestId("app-markdown-display-block").first(),
    ).toBeVisible({ timeout: 10000 });

    // Verify DataTableBlock is present
    await expect(page.getByTestId("app-data-table-block").first()).toBeVisible({
      timeout: 10000,
    });

    // Verify LineChart is present (there may be multiple)
    await expect(page.getByTestId("app-line-chart").first()).toBeVisible({
      timeout: 10000,
    });

    // Verify BarChart is present (there may be multiple)
    await expect(page.getByTestId("app-bar-chart").first()).toBeVisible({
      timeout: 10000,
    });
  });
});
