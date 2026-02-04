import { expect, test } from "@playwright/test";

test.describe("Navigation", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");
  });

  test("should navigate to home page", async ({ page }) => {
    await page.goto("/threads");

    // Navigate back to home
    await page.getByRole("link", { name: "Home" }).click();

    // Verify we're on the home page
    await expect(page).toHaveURL(/\/(home)?$/);
    await expect(page.getByRole("textbox", { name: "Ask anything" })).toBeVisible();
  });

  test("should navigate to threads page", async ({ page }) => {
    // Click on Threads link
    await page.getByRole("link", { name: "Threads" }).click();

    // Verify navigation
    await expect(page).toHaveURL(/\/threads/);
    await expect(page.getByRole("heading", { name: "Threads", level: 1 })).toBeVisible();
  });

  test("should navigate to ontology page", async ({ page }) => {
    // Click on Ontology link - scroll into view first to ensure it's clickable
    const ontologyLink = page.getByRole("link", { name: "Ontology" });
    await ontologyLink.scrollIntoViewIfNeeded();
    await ontologyLink.click({ force: true });

    // Verify navigation
    await expect(page).toHaveURL(/\/ontology/);

    // Wait for ontology graph to load - use data-testid instead of text that may appear later
    await expect(page.getByTestId("ontology-graph-container")).toBeVisible({
      timeout: 10000
    });

    // Verify the graph has actually loaded by checking for the overview text
    await expect(page.getByText("Ontology Overview")).toBeVisible({
      timeout: 10000
    });
  });

  test("should navigate to IDE page", async ({ page }) => {
    // Click on Dev Mode link
    await page.getByRole("link", { name: "Dev Mode" }).click();

    // Verify navigation
    await expect(page).toHaveURL(/\/ide/);
    await expect(page.getByText("Files").first()).toBeVisible();
    await expect(page.getByText("Local mode")).toBeVisible();
  });

  test("should navigate to thread detail from sidebar", async ({ page }) => {
    // Wait for threads to load in sidebar and click on first thread
    const firstThread = page.locator('[data-testid^="sidebar-thread-link-"]').first();
    await expect(firstThread).toBeVisible({ timeout: 10000 });
    await firstThread.click();

    // Verify we're on a thread detail page
    await expect(page).toHaveURL(/\/threads\/.+/);
    await expect(page.getByRole("textbox", { name: "Ask a follow-up question..." })).toBeVisible();
  });

  test("should navigate to workflow page from sidebar", async ({ page }) => {
    // Ensure sidebar workflows are loaded/expanded
    const showAllButton = page.getByRole("button", {
      name: /Show all.*automations/i
    });
    if (await showAllButton.isVisible().catch(() => false)) {
      await showAllButton.click();
    }

    // Prefer specific workflow link, otherwise fallback to first available
    const specificWorkflow = page.getByTestId("workflow-link-fruit_sales_report");
    const anyWorkflow = page.locator('[data-testid^="workflow-link-"]').first();

    const target = (await specificWorkflow.count()) > 0 ? specificWorkflow : anyWorkflow;

    // Wait for the workflow link to be visible and clickable
    await expect(target).toBeVisible({ timeout: 15000 });
    await target.scrollIntoViewIfNeeded();
    await target.click({ timeout: 15000 });

    // Verify navigation to workflow page
    await expect(page).toHaveURL(/\/workflows\/.+/);
  });

  test("should navigate to app page from sidebar", async ({ page }) => {
    // Find and click on an app link
    const appLink = page.locator('[data-testid^="app-link-"]').first();
    await expect(appLink).toBeVisible({ timeout: 10000 });
    await appLink.click();

    // Verify navigation to app page
    await expect(page).toHaveURL(/\/apps\/.+/);
  });

  test("should maintain sidebar state across navigation", async ({ page }) => {
    // Verify sidebar is visible
    await expect(page.getByRole("link", { name: "Home" })).toBeVisible();

    // Navigate to different pages
    await page.getByRole("link", { name: "Threads" }).click();
    await page.waitForLoadState("networkidle");
    await expect(page.getByRole("link", { name: "Home" })).toBeVisible();

    await page.getByRole("link", { name: "Dev Mode" }).click();
    await page.waitForLoadState("networkidle");
    await expect(page.getByRole("link", { name: "Home" })).toBeVisible();

    // Navigate to Ontology page directly since the link may be outside viewport on IDE page
    await page.goto("/ontology");
    await page.waitForLoadState("networkidle");
    await expect(page.getByRole("link", { name: "Home" })).toBeVisible();
  });
});
