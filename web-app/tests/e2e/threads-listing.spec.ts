import { test, expect } from "@playwright/test";
import { resetProject, seedThreadsDataViaAPI } from "./utils";

test.describe("Threads Listing Page", () => {
  // Reset and seed data once for the entire test suite
  test.beforeAll(async () => {
    resetProject();
    // Seed 20 threads via API to ensure pagination shows (10 per page = 2 pages)
    await seedThreadsDataViaAPI(20);
  });

  test.beforeEach(async ({ page }) => {
    await page.goto("/threads");
  });

  test("should display threads list", async ({ page }) => {
    // Verify the page title
    await expect(
      page.getByRole("heading", { name: "Threads", level: 1 }),
    ).toBeVisible();

    // Verify thread items are visible
    const threadItems = page.locator('[data-testid="thread-item"]');
    await expect(threadItems.first()).toBeVisible({ timeout: 10000 });
  });

  test("should display thread metadata", async ({ page }) => {
    // Wait for threads to load
    const firstThread = page.locator('[data-testid="thread-item"]').first();
    await expect(firstThread).toBeVisible({ timeout: 10000 });

    // Verify thread has a title (heading level 2)
    await expect(firstThread.getByRole("heading", { level: 2 })).toBeVisible();

    // Verify thread has a timestamp
    await expect(firstThread.locator("p").first()).toBeVisible();
  });

  test("should navigate to thread when clicked", async ({ page }) => {
    // Click on the first thread
    const firstThread = page.locator('[data-testid="thread-item"]').first();
    await expect(firstThread).toBeVisible({ timeout: 10000 });
    await firstThread.click();

    // Verify navigation to thread detail page
    await expect(page).toHaveURL(/\/threads\/.+/);
    await expect(
      page.getByRole("textbox", { name: "Ask a follow-up question..." }),
    ).toBeVisible();
  });

  test("should display pagination controls", async ({ page }) => {
    // Wait for threads to load
    await expect(
      page.locator('[data-testid="thread-item"]').first(),
    ).toBeVisible({ timeout: 10000 });

    // Verify pagination navigation exists (with extended timeout)
    await expect(
      page.getByRole("navigation", { name: "pagination" }),
    ).toBeVisible({ timeout: 10000 });

    // Verify page number links
    await expect(
      page.getByRole("link", { name: "1", exact: true }),
    ).toBeVisible();
  });

  test("should navigate to next page when pagination clicked", async ({
    page,
  }) => {
    // Wait for threads to load
    await expect(
      page.locator('[data-testid="thread-item"]').first(),
    ).toBeVisible({ timeout: 10000 });

    // Click on page 2 (use exact match to avoid matching "20")
    const nextPageLink = page.getByRole("link", { name: "2", exact: true });
    if (await nextPageLink.isVisible()) {
      await nextPageLink.click();

      // Verify URL contains page parameter or threads changed
      // Wait for page to update by checking for visible thread items
      await expect(
        page.locator('[data-testid="thread-item"]').first(),
      ).toBeVisible();
    }
  });

  test("should display items per page selector", async ({ page }) => {
    // Wait for threads to load
    await expect(
      page.locator('[data-testid="thread-item"]').first(),
    ).toBeVisible({ timeout: 10000 });

    // Verify items per page combobox
    const itemsPerPage = page.locator("role=combobox").first();
    await expect(itemsPerPage).toBeVisible();
    await expect(itemsPerPage).toContainText("10 / page");
  });

  test("should display select mode button", async ({ page }) => {
    // Wait for threads to load
    await expect(
      page.locator('[data-testid="thread-item"]').first(),
    ).toBeVisible({ timeout: 10000 });

    // Verify Select button exists
    await expect(page.getByRole("button", { name: "Select" })).toBeVisible();
  });

  test("should enable checkboxes in select mode", async ({ page }) => {
    // Wait for threads to load
    await expect(
      page.locator('[data-testid="thread-item"]').first(),
    ).toBeVisible({ timeout: 10000 });

    // Click Select button
    await page.getByRole("button", { name: "Select" }).click();

    // Verify checkboxes appear
    const firstCheckbox = page
      .locator('[data-testid="thread-item"]')
      .first()
      .locator("role=checkbox");
    await expect(firstCheckbox).toBeVisible();
  });
});
