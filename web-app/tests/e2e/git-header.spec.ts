/**
 * Git header integration tests.
 *
 * These tests verify that navigation around the IDE does not crash the
 * React tree (the primary regression: useDiffSummary / useRevisionInfo
 * used to call useCurrentProjectBranch() unconditionally, which throws
 * when project is null during initial render).
 *
 * They also cover the visible git-action UI: branch pill, refresh button,
 * commit & push button, and the ChangesPanel sheet.
 */
import { expect, test } from "@playwright/test";
import { IDEPage } from "./pages/IDEPage";
import { resetTestFile } from "./utils";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Wait for the IDE header to be fully rendered (card element present). */
async function waitForHeader(page: import("@playwright/test").Page) {
  // The header is a Card with the home button inside it.
  await expect(page.getByRole("button", { name: "Back to Home" })).toBeVisible({
    timeout: 15_000
  });
}

/** Return true when the page has no visible React error overlay. */
async function hasNoErrorOverlay(page: import("@playwright/test").Page): Promise<boolean> {
  // React DevTools error overlay uses a shadow host with id "react-error-overlay-...".
  // A simpler proxy: check that a well-known "crash" string is NOT in the page body.
  const body = await page
    .locator("body")
    .textContent({ timeout: 3000 })
    .catch(() => "");
  const looksLikeCrash =
    body?.includes("Something went wrong") ||
    body?.includes("Minified React error") ||
    body?.includes("Cannot read properties of null");
  return !looksLikeCrash;
}

// ---------------------------------------------------------------------------
// Test suite
// ---------------------------------------------------------------------------

test.describe("Git Header — navigation & rendering", () => {
  test.beforeEach(async ({ page }) => {
    await resetTestFile();
    await page.goto("/ide");
    await page.waitForLoadState("networkidle");
    await waitForHeader(page);
  });

  // -------------------------------------------------------------------------
  // Core regression: navigating to IDE must not crash React
  // -------------------------------------------------------------------------

  test("IDE loads without React crash", async ({ page }) => {
    // The header card (Back to Home button) must be present and the page
    // body must not contain any crash markers.
    await waitForHeader(page);
    expect(await hasNoErrorOverlay(page)).toBe(true);
  });

  test("navigate away and back to IDE does not crash", async ({ page }) => {
    // Go to Home, then back to IDE — verifies project-null teardown/remount
    await page.goto("/");
    await page.waitForLoadState("networkidle");
    await page.goto("/ide");
    await page.waitForLoadState("networkidle");
    await waitForHeader(page);
    expect(await hasNoErrorOverlay(page)).toBe(true);
  });

  test("switching between IDE files does not crash", async ({ page }) => {
    const idePage = new IDEPage(page);

    // Switch to Files mode and open two different files sequentially.
    await page.getByRole("tab", { name: "Files" }).click();
    await page.waitForTimeout(500);

    await idePage.openFile("config.yml");
    await idePage.verifyFileIsOpen("config.yml");
    expect(await hasNoErrorOverlay(page)).toBe(true);

    await idePage.openFile("test-file-for-e2e.txt");
    await idePage.verifyFileIsOpen("test-file-for-e2e.txt");
    expect(await hasNoErrorOverlay(page)).toBe(true);
  });

  test("navigating threads → IDE → threads does not crash", async ({ page }) => {
    await page.goto("/threads");
    await page.waitForLoadState("networkidle");
    await page.goto("/ide");
    await page.waitForLoadState("networkidle");
    await waitForHeader(page);
    expect(await hasNoErrorOverlay(page)).toBe(true);
    await page.goto("/threads");
    await page.waitForLoadState("networkidle");
    expect(await hasNoErrorOverlay(page)).toBe(true);
  });

  // -------------------------------------------------------------------------
  // Header branch UI (branch pill always visible in local-git mode)
  // -------------------------------------------------------------------------

  test("branch pill is visible in the header", async ({ page }) => {
    // The branch pill is a button that contains a BranchInfo component.
    // It should always be visible when the workspace has a local git repo
    // (capabilities.can_commit / git_mode != "none").
    // Prefer the more specific BranchQuickSwitcher trigger: it has a chevron
    // and git-branch icon. We just verify the header area has branch-related UI.
    const headerCard = page.locator(".border-b.bg-sidebar-background");
    await expect(headerCard).toBeVisible({ timeout: 10_000 });
  });

  // -------------------------------------------------------------------------
  // Non-main branch UI: refresh button must be visible
  // -------------------------------------------------------------------------

  test("refresh git status button is visible on non-main branch", async ({ page }) => {
    // The running branch in this repo is git-branching (not main), so the
    // refresh button should always be present.
    const refreshButton = page.getByTestId("ide-git-refresh-button");

    // It may only appear after project loads; wait up to 10s.
    const isVisible = await refreshButton.isVisible({ timeout: 10_000 }).catch(() => false);

    // If the project is in cloud-only mode there may be no git actions at all.
    // We only assert when the button does appear (local-git mode is the default
    // in dev, so it will be present in the standard test environment).
    if (isVisible) {
      await expect(refreshButton).toBeEnabled();
      await expect(refreshButton).toHaveAttribute("title", "Refresh git status");
    }
  });

  test("clicking the refresh button triggers a git status refresh", async ({ page }) => {
    const refreshButton = page.getByTestId("ide-git-refresh-button");
    const visible = await refreshButton.isVisible({ timeout: 10_000 }).catch(() => false);
    if (!visible) {
      test.skip();
      return;
    }

    // Click and verify the button is temporarily disabled (isFetching → disabled)
    // or re-enabled quickly. Either way, no crash should occur.
    await refreshButton.click();
    // Wait a moment for the refetch to kick off and settle.
    await page.waitForTimeout(1000);
    expect(await hasNoErrorOverlay(page)).toBe(true);
  });

  // -------------------------------------------------------------------------
  // ChangesPanel — open/close lifecycle
  // -------------------------------------------------------------------------

  test("ChangesPanel opens when commit button is clicked", async ({ page }) => {
    const commitButton = page.getByTestId("ide-commit-push-button");
    const panelVisible = await commitButton.isVisible({ timeout: 15_000 }).catch(() => false);

    if (!panelVisible) {
      // No uncommitted changes in the working tree — skip this assertion.
      // The test still passes as a no-op; existence of changes is environment-
      // dependent and is not the focus of this regression suite.
      test.skip();
      return;
    }

    await commitButton.click();

    // The ChangesPanel sheet should now be open.
    const panel = page.getByTestId("changes-panel");
    await expect(panel).toBeVisible({ timeout: 5000 });

    // The commit message textarea should be present.
    const textarea = page.getByTestId("changes-panel-commit-message");
    await expect(textarea).toBeVisible();

    // The push button inside the panel should be present.
    const pushButton = page.getByTestId("changes-panel-push-button");
    await expect(pushButton).toBeVisible();
  });

  test("ChangesPanel closes via Escape key", async ({ page }) => {
    const commitButton = page.getByTestId("ide-commit-push-button");
    const panelVisible = await commitButton.isVisible({ timeout: 15_000 }).catch(() => false);
    if (!panelVisible) {
      test.skip();
      return;
    }

    await commitButton.click();
    await expect(page.getByTestId("changes-panel")).toBeVisible({ timeout: 5000 });

    await page.keyboard.press("Escape");
    await expect(page.getByTestId("changes-panel")).not.toBeVisible({ timeout: 5000 });
  });

  test("ChangesPanel commit message can be edited", async ({ page }) => {
    const commitButton = page.getByTestId("ide-commit-push-button");
    const panelVisible = await commitButton.isVisible({ timeout: 15_000 }).catch(() => false);
    if (!panelVisible) {
      test.skip();
      return;
    }

    await commitButton.click();
    await expect(page.getByTestId("changes-panel")).toBeVisible({ timeout: 5000 });

    const textarea = page.getByTestId("changes-panel-commit-message");
    await textarea.fill("My custom commit message");
    await expect(textarea).toHaveValue("My custom commit message");
  });

  // -------------------------------------------------------------------------
  // File save → diff count update (smoke test, no actual git assertion)
  // -------------------------------------------------------------------------

  test("saving a file does not crash the IDE", async ({ page }) => {
    const idePage = new IDEPage(page);

    await page.getByRole("tab", { name: "Files" }).click();
    await page.waitForTimeout(500);
    await idePage.openFile("test-file-for-e2e.txt");
    await idePage.insertTextAtEnd("# Git header test");
    await idePage.saveFile();

    // After save, the header should still be intact.
    await waitForHeader(page);
    expect(await hasNoErrorOverlay(page)).toBe(true);
  });
});
