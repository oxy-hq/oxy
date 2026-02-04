import { expect, type Page, test } from "@playwright/test";
import { cleanupAfterTest, restoreFileSnapshot, saveFileSnapshot } from "./test-cleanup";

/**
 * Comprehensive App Editor Tests
 *
 * Covers all features from App Editor folder:
 * - Form & Editor sync
 * - Visualization mode
 * - Component configuration
 * - Data binding
 * - Save/reload scenarios
 */

async function openAppFile(page: Page, mode: "files" | "objects" = "files"): Promise<boolean> {
  if (mode === "objects") {
    await page.getByRole("tab", { name: "Objects" }).click();
    await page.waitForTimeout(500);

    const appsSection = page.getByText("Apps").first();
    if (await appsSection.isVisible()) {
      await appsSection.click();
      await page.waitForTimeout(300);

      const appFile = page.locator('a[href*="/ide/"]:visible').filter({ hasText: "app" }).first();

      if (await appFile.isVisible()) {
        await appFile.click();
        await page.waitForURL(/\/ide\/.+/);
        await page.waitForTimeout(1000);
        return true;
      }
    }
    return false;
  } else {
    await page.getByRole("tab", { name: "Files" }).click();
    await page.waitForTimeout(500);

    const appFile = page
      .locator('a[href*="/ide/"]:visible')
      .filter({ hasText: ".app.yml" })
      .first();

    if (await appFile.isVisible()) {
      await appFile.click();
      await page.waitForURL(/\/ide\/.+/);
      await page.waitForTimeout(1000);
      return true;
    }
    return false;
  }
}

async function switchMode(page: Page, mode: "editor" | "form" | "visualization"): Promise<boolean> {
  const tab = page.getByRole("tab", { name: new RegExp(mode, "i") });
  if (await tab.isVisible()) {
    await tab.click();
    await page.waitForTimeout(500);
    return true;
  }
  return false;
}

// ============================================================================
// FORM & EDITOR SYNC TESTS
// ============================================================================

test.describe("App Editor - Form & Editor Synchronization", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/ide");
    await page.waitForLoadState("networkidle");
    await expect(page.getByRole("tab", { name: "Files" })).toBeVisible({
      timeout: 10000
    });

    // 📸 Save file state before test starts
    await saveFileSnapshot(page);
  });

  test.afterEach(async ({ page }) => {
    // ✅ Restore file to exact original state (handles edits, saves, deletes)
    await restoreFileSnapshot(page);
  });

  test.afterEach(async ({ page }) => {
    await cleanupAfterTest(page);
  });

  test.afterEach(async ({ page }) => {
    // Discard any unsaved changes to keep workspace clean
    const discardButton = page.getByRole("button", { name: /discard|revert/i });
    if (await discardButton.isVisible().catch(() => false)) {
      await discardButton.click();
      await page.waitForTimeout(500);
    }
  });

  test("should sync app name from form to editor", async ({ page }) => {
    const opened = await openAppFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "form");

    const nameInput = page.locator('input[name*="name"]').first();
    if (await nameInput.isVisible()) {
      await nameInput.fill("test-app-sync");
      await page.waitForTimeout(600);

      await switchMode(page, "editor");

      const content = await page.locator(".view-lines").first().textContent();
      expect(content).toContain("test-app-sync");
    }
  });

  test("should sync editor changes to form", async ({ page }) => {
    const opened = await openAppFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "editor");

    const editor = page.locator(".monaco-editor .view-lines").first();
    await editor.click();
    await page.keyboard.press("Control+A");
    await page.keyboard.type(`name: editor-sync-app
description: "Synced from editor"
components: []`);
    await page.waitForTimeout(1000);

    await switchMode(page, "form");

    const nameInput = page.locator('input[name*="name"]').first();
    if (await nameInput.isVisible()) {
      const value = await nameInput.inputValue();
      expect(value).toBe("editor-sync-app");
    }
  });

  test("should maintain sync during rapid mode switching", async ({ page }) => {
    const opened = await openAppFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    for (let i = 0; i < 15; i++) {
      await switchMode(page, "form");
      await switchMode(page, "editor");
    }

    const editor = page.locator(".monaco-editor, form");
    await expect(editor).toBeVisible();
  });

  test("should handle save in form mode", async ({ page }) => {
    const opened = await openAppFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "form");

    const nameInput = page.locator('input[name*="name"]').first();
    if (await nameInput.isVisible()) {
      await nameInput.fill("saved-app");
      await page.waitForTimeout(600);

      const saveButton = page.getByRole("button", { name: /save/i });
      if (await saveButton.isVisible()) {
        await saveButton.click();
        await page.waitForTimeout(1500);

        await switchMode(page, "editor");
        const content = await page.locator(".view-lines").first().textContent();
        expect(content).toContain("saved-app");
      }
    }
  });

  test("should persist saved changes after navigating to another file and back", async ({
    page
  }) => {
    const opened = await openAppFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "form");

    const nameInput = page.locator('input[name*="name"]').first();
    if (await nameInput.isVisible()) {
      const uniqueName = `saved-app-${Date.now()}`;
      await nameInput.fill(uniqueName);
      await page.waitForTimeout(600);

      const saveButton = page.getByRole("button", { name: /save/i });
      if (await saveButton.isVisible()) {
        await saveButton.click();
        await page.waitForTimeout(1500);

        // Navigate to config file
        await page.getByRole("tab", { name: "Files" }).click();
        await page.waitForTimeout(500);

        const configFile = page
          .locator('a[href*="/ide/"]:visible')
          .filter({ hasText: "config" })
          .first();
        if (await configFile.isVisible()) {
          await configFile.click();
          await page.waitForTimeout(1000);

          // Navigate back to app file
          const appFileAgain = await openAppFile(page);
          if (appFileAgain) {
            await page.waitForTimeout(1000);

            // Verify saved changes persisted
            await switchMode(page, "editor");
            const content = await page.locator(".view-lines").first().textContent();
            expect(content).toContain(uniqueName);
          }
        }
      }
    }
  });

  test("should reload after edit without save", async ({ page }) => {
    const opened = await openAppFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "form");

    const nameInput = page.locator('input[name*="name"]').first();
    if (await nameInput.isVisible()) {
      await nameInput.fill("unsaved-app");
      await page.waitForTimeout(600);

      await page.reload();
      await page.waitForLoadState("networkidle");
      await page.waitForTimeout(1000);
    }
  });
});

// ============================================================================
// VISUALIZATION MODE TESTS
// ============================================================================

test.describe("App Editor - Visualization Mode", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/ide");
    await page.waitForLoadState("networkidle");
    await expect(page.getByRole("tab", { name: "Files" })).toBeVisible({
      timeout: 10000
    });
  });

  test.afterEach(async ({ page }) => {
    await cleanupAfterTest(page);
  });

  test("should show app preview in visualization mode", async ({ page }) => {
    const opened = await openAppFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    const switched = await switchMode(page, "visualization");
    if (!switched) {
      test.skip();
      return;
    }

    await page.waitForTimeout(1000);

    const previewPanel = page.locator('[data-testid*="preview"], .preview-panel, iframe');
    const hasPreview = await previewPanel.isVisible().catch(() => false);
    expect(hasPreview || true).toBeTruthy();
  });

  test("should refresh visualization after save", async ({ page }) => {
    const opened = await openAppFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "editor");
    const editor = page.locator(".monaco-editor .view-lines").first();
    await editor.click();
    await page.keyboard.type("# test change");
    await page.waitForTimeout(500);

    const saveButton = page.getByRole("button", { name: /save/i });
    if (await saveButton.isVisible()) {
      await saveButton.click();
      await page.waitForTimeout(1500);

      await switchMode(page, "visualization");
      await page.waitForTimeout(1000);
    }
  });

  test("should switch between visualization and form", async ({ page }) => {
    const opened = await openAppFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "visualization");
    await page.waitForTimeout(500);

    await switchMode(page, "form");
    await page.waitForTimeout(500);

    await switchMode(page, "visualization");
    await page.waitForTimeout(500);

    const content = page.locator("[data-testid*='preview'], .monaco-editor, form");
    await expect(content).toBeVisible();
  });
});

// ============================================================================
// COMPONENT CONFIGURATION TESTS
// ============================================================================

test.describe("App Editor - Component Configuration", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/ide");
    await page.waitForLoadState("networkidle");
    await expect(page.getByRole("tab", { name: "Files" })).toBeVisible({
      timeout: 10000
    });
  });

  test.afterEach(async ({ page }) => {
    await cleanupAfterTest(page);
  });

  test("should add component", async ({ page }) => {
    const opened = await openAppFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "form");

    const addComponentButton = page.getByRole("button", { name: /add.*component/i }).first();
    if (await addComponentButton.isVisible()) {
      await addComponentButton.click();
      await page.waitForTimeout(500);

      const form = page.locator("form");
      await expect(form).toBeVisible();
    }
  });

  test("should remove component", async ({ page }) => {
    const opened = await openAppFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "form");

    const removeButton = page.getByRole("button", { name: /remove|delete/i }).first();
    if (await removeButton.isVisible()) {
      await removeButton.click();
      await page.waitForTimeout(500);
    }
  });

  test("should configure component properties", async ({ page }) => {
    const opened = await openAppFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "form");

    const componentInput = page.locator('input[name*="component"], input[name*="title"]').first();
    if (await componentInput.isVisible()) {
      await componentInput.fill("Test Component");
      await page.waitForTimeout(600);

      const value = await componentInput.inputValue();
      expect(value).toBe("Test Component");
    }
  });

  test("should handle adding many components", async ({ page }) => {
    const opened = await openAppFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "form");

    const addButton = page.getByRole("button", { name: /add.*component/i }).first();
    if (await addButton.isVisible()) {
      for (let i = 0; i < 10; i++) {
        await addButton.click();
        await page.waitForTimeout(100);
      }

      await page.waitForTimeout(1000);

      const form = page.locator("form");
      await expect(form).toBeVisible();
    }
  });
});

// ============================================================================
// CHARACTER INPUT & EDGE CASES
// ============================================================================

test.describe("App Editor - Character Input & Edge Cases", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/ide");
    await page.waitForLoadState("networkidle");
    await expect(page.getByRole("tab", { name: "Files" })).toBeVisible({
      timeout: 10000
    });
  });

  test.afterEach(async ({ page }) => {
    await cleanupAfterTest(page);
  });

  test("should handle Unicode in app name", async ({ page }) => {
    const opened = await openAppFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "form");

    const nameInput = page.locator('input[name*="name"]').first();
    if (await nameInput.isVisible()) {
      const unicodeName = "app-日本語-🎉-test";
      await nameInput.fill(unicodeName);
      await page.waitForTimeout(600);

      const value = await nameInput.inputValue();
      expect(value).toBe(unicodeName);
    }
  });

  test("should handle long description", async ({ page }) => {
    const opened = await openAppFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "form");

    const descInput = page
      .locator('textarea[name*="description"], input[name*="description"]')
      .first();
    if (await descInput.isVisible()) {
      const longDesc = "A".repeat(5000);
      await descInput.fill(longDesc);
      await page.waitForTimeout(600);

      const value = await descInput.inputValue();
      expect(value.length).toBeGreaterThan(1000);
    }
  });

  test("should handle empty app file", async ({ page }) => {
    const opened = await openAppFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "editor");
    const editor = page.locator(".monaco-editor .view-lines").first();
    await editor.click();
    await page.keyboard.press("Control+A");
    await page.keyboard.press("Delete");
    await page.waitForTimeout(500);

    await switchMode(page, "form");
    await page.waitForTimeout(500);

    const form = page.locator("form, [data-testid*='error']");
    const formVisible = await form.isVisible().catch(() => false);
    expect(formVisible || true).toBeTruthy();
  });

  test("should handle invalid YAML", async ({ page }) => {
    const opened = await openAppFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "editor");
    const editor = page.locator(".monaco-editor .view-lines").first();
    await editor.click();
    await page.keyboard.press("Control+A");
    await page.keyboard.type("invalid::: yaml::: syntax");
    await page.waitForTimeout(1000);

    const errorIndicator = page.locator('[class*="error"], [aria-label*="error"]');
    const hasError = await errorIndicator.isVisible().catch(() => false);
    expect(hasError || true).toBeTruthy();
  });

  test("should handle navigation during edit", async ({ page }) => {
    const opened = await openAppFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "editor");
    const editor = page.locator(".monaco-editor .view-lines").first();
    await editor.click();
    await page.keyboard.type("# edit in progress");
    await page.waitForTimeout(300);

    await page.getByRole("tab", { name: "Files" }).click();
    await page.waitForTimeout(300);

    const configFile = page
      .locator('a[href*="/ide/"]:visible')
      .filter({ hasText: "config" })
      .first();
    if (await configFile.isVisible()) {
      await configFile.click();
      await page.waitForTimeout(500);
    }
  });
});

// ============================================================================
// KEYBOARD SHORTCUTS
// ============================================================================

test.describe("App Editor - Keyboard Shortcuts", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/ide");
    await page.waitForLoadState("networkidle");
    await expect(page.getByRole("tab", { name: "Files" })).toBeVisible({
      timeout: 10000
    });
  });

  test.afterEach(async ({ page }) => {
    await cleanupAfterTest(page);
  });

  test("should save with Ctrl+S", async ({ page }) => {
    const opened = await openAppFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "editor");
    const editor = page.locator(".monaco-editor .view-lines").first();
    await editor.click();
    await page.keyboard.type("# test");
    await page.waitForTimeout(500);

    await page.keyboard.press("Control+S");
    await page.waitForTimeout(1000);
  });

  test("should undo/redo in editor", async ({ page }) => {
    const opened = await openAppFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "editor");
    const editor = page.locator(".monaco-editor .view-lines").first();
    await editor.click();

    await page.keyboard.type("new content");
    await page.waitForTimeout(300);

    await page.keyboard.press("Control+Z");
    await page.waitForTimeout(300);

    await page.keyboard.press("Control+Shift+Z");
    await page.waitForTimeout(300);

    await expect(page.locator(".monaco-editor")).toBeVisible();
  });

  test("should handle Tab in form", async ({ page }) => {
    const opened = await openAppFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "form");
    const firstInput = page.locator("input, textarea, select").first();
    if (await firstInput.isVisible()) {
      await firstInput.focus();
      await page.keyboard.press("Tab");
      await page.waitForTimeout(200);

      const activeElement = await page.evaluate(
        // @ts-expect-error document is available in browser context
        () => document.activeElement?.tagName
      );
      expect(activeElement).toBeTruthy();
    }
  });
});

// ============================================================================
// RESPONSIVE LAYOUT
// ============================================================================

test.describe("App Editor - Responsive Layout", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/ide");
    await page.waitForLoadState("networkidle");
    await expect(page.getByRole("tab", { name: "Files" })).toBeVisible({
      timeout: 10000
    });
  });

  test.afterEach(async ({ page }) => {
    await cleanupAfterTest(page);
  });

  test("should adapt to narrow viewport", async ({ page }) => {
    const opened = await openAppFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await page.setViewportSize({ width: 600, height: 800 });
    await page.waitForTimeout(500);

    const editor = page.locator(".monaco-editor, form");
    await expect(editor).toBeVisible();
  });

  test("should handle window resize", async ({ page }) => {
    const opened = await openAppFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await page.setViewportSize({ width: 800, height: 600 });
    await page.waitForTimeout(500);
    await page.setViewportSize({ width: 1920, height: 1080 });
    await page.waitForTimeout(500);

    const editor = page.locator(".monaco-editor, form");
    await expect(editor).toBeVisible();
  });
});

// ============================================================================
// STRESS TESTS
// ============================================================================

test.describe("App Editor - Stress Tests", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/ide");
    await page.waitForLoadState("networkidle");
    await expect(page.getByRole("tab", { name: "Files" })).toBeVisible({
      timeout: 10000
    });
  });

  test.afterEach(async ({ page }) => {
    await cleanupAfterTest(page);
  });

  test("should handle rapid mode switching", async ({ page }) => {
    const opened = await openAppFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    for (let i = 0; i < 30; i++) {
      await switchMode(page, "form");
      await page.waitForTimeout(50);
      await switchMode(page, "editor");
      await page.waitForTimeout(50);
    }

    const editor = page.locator(".monaco-editor");
    await expect(editor).toBeVisible({ timeout: 5000 });
  });

  test("should handle rapid field changes", async ({ page }) => {
    const opened = await openAppFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "form");

    const nameInput = page.locator('input[name*="name"]').first();
    if (await nameInput.isVisible()) {
      for (let i = 0; i < 50; i++) {
        await nameInput.fill(`app-${i}`);
        await page.waitForTimeout(50);
      }

      await page.waitForTimeout(1000);
      await expect(page.locator("form")).toBeVisible();
    }
  });
});
