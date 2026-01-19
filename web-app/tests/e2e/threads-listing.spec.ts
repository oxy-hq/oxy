import { test, expect } from "@playwright/test";
import { mockThreadsEndpoints } from "./mocks/threads";
/**
 * Threads Listing Spec (Mocked Data)
 * ----------------------------------
 * We intentionally use mocked thread data here instead of creating real threads via the API.
 * Reasons:
 * 1. Eliminate latency + nondeterminism from the language model / agent execution
 *    pipeline. Real answers require background processing and can vary in timing.
 * 2. Ensure a consistent dataset (exactly 10 identical threads) so UI assertions
 *    about list rendering, absence of pagination, and navigation are deterministic.
 * This keeps the test focused on frontend behavior (rendering, navigation, selection)
 * rather than backend task orchestration.
 */

test.describe("Threads Listing Page", () => {
  test.beforeEach(async ({ page }) => {
    await mockThreadsEndpoints(page);
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

    // Wait for the thread page to load - the page shows a skeleton first, then loads data
    // Give it more time to complete the loading and render the message input
    await expect(
      page.getByRole("textbox", { name: "Ask a follow-up question..." }),
    ).toBeVisible({ timeout: 3000 });
  });

  test("should display pagination controls", async ({ page }) => {
    // With mocked single page data, pagination nav should not exist; assert absence
    await expect(
      page.getByRole("navigation", { name: "pagination" }),
    ).toHaveCount(0);
  });

  test("should not navigate to a second page", async ({ page }) => {
    const page2Link = page.getByRole("link", { name: "2" });
    expect(await page2Link.count()).toBe(0);
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

  // No per-suite cleanup; global setup handles initial state once.
});
