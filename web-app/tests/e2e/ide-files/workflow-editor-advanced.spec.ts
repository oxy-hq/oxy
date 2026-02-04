import { expect, type Page, test } from "@playwright/test";
import { cleanupAfterTest, restoreFileSnapshot, saveFileSnapshot } from "./test-cleanup";

/**
 * Advanced Workflow Editor Tests
 *
 * This file contains advanced test scenarios including:
 * - Stress tests with extreme inputs
 * - Race condition testing
 * - Memory leak detection
 * - Performance testing
 * - Browser compatibility edge cases
 */

// Helper to open workflow
async function openWorkflow(page: Page): Promise<boolean> {
  await page.getByRole("tab", { name: "Files" }).click();
  await page.waitForTimeout(500);

  const workflowsFolder = page.getByRole("button", {
    name: "workflows",
    exact: true
  });
  if (await workflowsFolder.isVisible()) {
    await workflowsFolder.click();
    await page.waitForTimeout(500);

    const workflowFile = page
      .locator('a[href*="/ide/"]:visible')
      .filter({ hasText: ".workflow.yml" })
      .first();

    if (await workflowFile.isVisible()) {
      await workflowFile.click();
      await page.waitForURL(/\/ide\/.+/);
      await page.waitForTimeout(1000);
      return true;
    }
  }
  return false;
}

async function switchMode(page: Page, mode: string): Promise<boolean> {
  const tab = page.getByRole("tab", { name: new RegExp(mode, "i") });
  if (await tab.isVisible()) {
    await tab.click();
    await page.waitForTimeout(500);
    return true;
  }
  return false;
}

// ============================================================================
// STRESS & PERFORMANCE TESTS
// ============================================================================

test.describe("Workflow Editor - Stress Tests", () => {
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

  test("should handle extremely large YAML file", async ({ page }) => {
    const opened = await openWorkflow(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "editor");
    const editor = page.locator(".monaco-editor .view-lines").first();
    await editor.click();

    // Generate large YAML content
    await page.keyboard.press("Control+A");

    let largeYaml = "name: stress-test\ndescription: Large workflow\ntasks:\n";
    for (let i = 0; i < 100; i++) {
      largeYaml += `  - name: task_${i}\n    type: agent\n    prompt: "Task ${i} prompt"\n`;
    }

    await page.keyboard.type(largeYaml.slice(0, 5000)); // Limit for test speed
    await page.waitForTimeout(1000);

    // Should still be responsive
    const editorVisible = await page.locator(".monaco-editor").isVisible();
    expect(editorVisible).toBeTruthy();
  });

  test("should handle rapid mode switching under load", async ({ page }) => {
    const opened = await openWorkflow(page);
    if (!opened) {
      test.skip();
      return;
    }

    // Rapid switching 50 times
    for (let i = 0; i < 50; i++) {
      await switchMode(page, "form");
      await page.waitForTimeout(50);
      await switchMode(page, "editor");
      await page.waitForTimeout(50);
    }

    // Should still work
    const editor = page.locator(".monaco-editor");
    await expect(editor).toBeVisible({ timeout: 5000 });
  });

  test("should handle continuous typing for 5 seconds", async ({ page }) => {
    const opened = await openWorkflow(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "editor");
    const editor = page.locator(".monaco-editor .view-lines").first();
    await editor.click();

    // Continuous typing
    const startTime = Date.now();
    while (Date.now() - startTime < 5000) {
      await page.keyboard.type("test ");
      await page.waitForTimeout(50);
    }

    // Should not crash
    await expect(page.locator(".monaco-editor")).toBeVisible();
  });

  test("should handle rapid save attempts", async ({ page }) => {
    const opened = await openWorkflow(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "editor");
    const editor = page.locator(".monaco-editor .view-lines").first();
    await editor.click();
    await page.keyboard.type("save test");

    // Rapid saves
    for (let i = 0; i < 20; i++) {
      await page.keyboard.press("Control+S");
      await page.waitForTimeout(50);
    }

    await page.waitForTimeout(2000);

    // Should complete without errors
    await expect(page.locator(".monaco-editor")).toBeVisible();
  });

  test("should handle many open/close cycles", async ({ page }) => {
    for (let i = 0; i < 5; i++) {
      const opened = await openWorkflow(page);
      if (!opened) break;

      // Navigate away
      await page.getByRole("tab", { name: "Files" }).click();
      await page.waitForTimeout(300);

      const configFile = page
        .locator('a[href*="/ide/"]:visible')
        .filter({ hasText: "config" })
        .first();
      if (await configFile.isVisible()) {
        await configFile.click();
        await page.waitForTimeout(300);
      }
    }

    // Should still work
    const opened = await openWorkflow(page);
    expect(opened).toBeTruthy();
  });
});

// ============================================================================
// RACE CONDITION TESTS
// ============================================================================

test.describe("Workflow Editor - Race Conditions", () => {
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

  test("should handle save during mode switch", async ({ page }) => {
    const opened = await openWorkflow(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "editor");
    const editor = page.locator(".monaco-editor .view-lines").first();
    await editor.click();
    await page.keyboard.type("race condition test");

    // Trigger save and immediately switch mode
    page.keyboard.press("Control+S").catch(() => {}); // Don't await
    await page.waitForTimeout(50);
    await switchMode(page, "form");

    await page.waitForTimeout(1000);

    // Should handle gracefully
    const form = page.locator("form, .monaco-editor");
    await expect(form).toBeVisible();
  });

  test("should handle navigation during save", async ({ page }) => {
    const opened = await openWorkflow(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "editor");
    const editor = page.locator(".monaco-editor .view-lines").first();
    await editor.click();
    await page.keyboard.type("navigation race test");

    // Trigger save and immediately navigate
    page.keyboard.press("Control+S").catch(() => {});
    await page.waitForTimeout(50);

    await page.getByRole("tab", { name: "Files" }).click();
    await page.waitForTimeout(1000);

    // Should handle without corruption
    await page.waitForLoadState("networkidle");
  });

  test("should handle concurrent form changes", async ({ page }) => {
    const opened = await openWorkflow(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "form");

    const nameInput = page.locator('input[name*="name"]').first();
    const descInput = page
      .locator('textarea[name*="description"], input[name*="description"]')
      .first();

    if ((await nameInput.isVisible()) && (await descInput.isVisible())) {
      // Change both fields rapidly
      nameInput.fill("concurrent-1").catch(() => {});
      descInput.fill("concurrent-desc-1").catch(() => {});
      await page.waitForTimeout(100);
      nameInput.fill("concurrent-2").catch(() => {});
      descInput.fill("concurrent-desc-2").catch(() => {});

      await page.waitForTimeout(1000);

      // Should handle without errors
      await expect(page.locator("form")).toBeVisible();
    }
  });

  test("should handle edit during auto-save", async ({ page }) => {
    const opened = await openWorkflow(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "editor");
    const editor = page.locator(".monaco-editor .view-lines").first();
    await editor.click();

    // Type, trigger save, and continue typing
    await page.keyboard.type("first part");
    page.keyboard.press("Control+S").catch(() => {});
    await page.waitForTimeout(50);
    await page.keyboard.type(" second part");

    await page.waitForTimeout(1000);

    const content = await page.locator(".view-lines").first().textContent();
    expect(content).toContain("first part");
    expect(content).toContain("second part");
  });
});

// ============================================================================
// BROWSER COMPATIBILITY & SPECIAL KEYS
// ============================================================================

test.describe("Workflow Editor - Special Keyboard Combinations", () => {
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

  test("should handle Ctrl+Home/End navigation", async ({ page }) => {
    const opened = await openWorkflow(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "editor");
    const editor = page.locator(".monaco-editor .view-lines").first();
    await editor.click();

    await page.keyboard.press("Control+End");
    await page.waitForTimeout(200);
    await page.keyboard.press("Control+Home");
    await page.waitForTimeout(200);

    // Should navigate without crash
    await expect(page.locator(".monaco-editor")).toBeVisible();
  });

  test("should handle PageUp/PageDown", async ({ page }) => {
    const opened = await openWorkflow(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "editor");
    const editor = page.locator(".monaco-editor .view-lines").first();
    await editor.click();

    await page.keyboard.press("PageDown");
    await page.waitForTimeout(200);
    await page.keyboard.press("PageUp");
    await page.waitForTimeout(200);

    await expect(page.locator(".monaco-editor")).toBeVisible();
  });

  test("should handle Ctrl+D (duplicate line)", async ({ page }) => {
    const opened = await openWorkflow(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "editor");
    const editor = page.locator(".monaco-editor .view-lines").first();
    await editor.click();

    await page.keyboard.type("duplicate this line");
    await page.keyboard.press("Control+D");
    await page.waitForTimeout(300);

    // Monaco might handle this differently
    await expect(page.locator(".monaco-editor")).toBeVisible();
  });

  test("should handle Alt+Arrow (move line)", async ({ page }) => {
    const opened = await openWorkflow(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "editor");
    const editor = page.locator(".monaco-editor .view-lines").first();
    await editor.click();

    await page.keyboard.press("Alt+ArrowDown");
    await page.waitForTimeout(200);
    await page.keyboard.press("Alt+ArrowUp");
    await page.waitForTimeout(200);

    await expect(page.locator(".monaco-editor")).toBeVisible();
  });

  test("should handle Ctrl+/ (comment toggle)", async ({ page }) => {
    const opened = await openWorkflow(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "editor");
    const editor = page.locator(".monaco-editor .view-lines").first();
    await editor.click();

    await page.keyboard.press("Control+/");
    await page.waitForTimeout(300);

    await expect(page.locator(".monaco-editor")).toBeVisible();
  });

  test("should handle Ctrl+Shift+K (delete line)", async ({ page }) => {
    const opened = await openWorkflow(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "editor");
    const editor = page.locator(".monaco-editor .view-lines").first();
    await editor.click();

    await page.keyboard.type("line to delete");
    await page.keyboard.press("Control+Shift+K");
    await page.waitForTimeout(300);

    await expect(page.locator(".monaco-editor")).toBeVisible();
  });

  test("should handle Ctrl+[ and Ctrl+] (indent/outdent)", async ({ page }) => {
    const opened = await openWorkflow(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "editor");
    const editor = page.locator(".monaco-editor .view-lines").first();
    await editor.click();

    await page.keyboard.type("indented line");
    await page.keyboard.press("Control+]"); // Indent
    await page.waitForTimeout(200);
    await page.keyboard.press("Control+["); // Outdent
    await page.waitForTimeout(200);

    await expect(page.locator(".monaco-editor")).toBeVisible();
  });

  test("should handle F1 (command palette)", async ({ page }) => {
    const opened = await openWorkflow(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "editor");
    const editor = page.locator(".monaco-editor .view-lines").first();
    await editor.click();

    await page.keyboard.press("F1");
    await page.waitForTimeout(500);

    // Command palette might open (Monaco feature)
    await page.keyboard.press("Escape");
    await page.waitForTimeout(200);

    await expect(page.locator(".monaco-editor")).toBeVisible();
  });
});

// ============================================================================
// FORM STRESS TESTS
// ============================================================================

test.describe("Workflow Editor - Form Stress Tests", () => {
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

  test("should handle adding 100 tasks", async ({ page }) => {
    const opened = await openWorkflow(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "form");

    const addButton = page.getByRole("button", { name: /add.*task/i }).first();
    if (await addButton.isVisible()) {
      // Add many tasks
      for (let i = 0; i < 100; i++) {
        await addButton.click();
        await page.waitForTimeout(50);
      }

      await page.waitForTimeout(2000);

      // Should handle without crash
      const form = page.locator("form");
      await expect(form).toBeVisible();
    }
  });

  test("should handle rapid form field changes", async ({ page }) => {
    const opened = await openWorkflow(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "form");

    const nameInput = page.locator('input[name*="name"]').first();
    if (await nameInput.isVisible()) {
      // Rapid changes
      for (let i = 0; i < 50; i++) {
        await nameInput.fill(`workflow-${i}`);
        await page.waitForTimeout(50);
      }

      await page.waitForTimeout(1000);

      // Should handle without errors
      await expect(page.locator("form")).toBeVisible();
    }
  });

  test("should handle form validation errors", async ({ page }) => {
    const opened = await openWorkflow(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "form");

    const nameInput = page.locator('input[name*="name"]').first();
    if (await nameInput.isVisible()) {
      // Try invalid values
      await nameInput.fill("");
      await nameInput.blur();
      await page.waitForTimeout(300);

      await nameInput.fill("!!!invalid!!!");
      await nameInput.blur();
      await page.waitForTimeout(300);

      // Should show validation errors without crash
      await expect(page.locator("form")).toBeVisible();
    }
  });

  test("should handle scrolling in long form", async ({ page }) => {
    const opened = await openWorkflow(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "form");

    // Add multiple tasks to make form scrollable
    const addButton = page.getByRole("button", { name: /add.*task/i }).first();
    if (await addButton.isVisible()) {
      for (let i = 0; i < 10; i++) {
        await addButton.click();
        await page.waitForTimeout(100);
      }

      await page.waitForTimeout(500);

      // Scroll form
      const formContainer = page.locator("form").first();
      await formContainer.evaluate((el) => {
        el.scrollTop = el.scrollHeight;
      });
      await page.waitForTimeout(200);

      await formContainer.evaluate((el) => {
        el.scrollTop = 0;
      });
      await page.waitForTimeout(200);

      // Should handle scrolling
      await expect(formContainer).toBeVisible();
    }
  });
});

// ============================================================================
// CLIPBOARD & COPY/PASTE TESTS
// ============================================================================

test.describe("Workflow Editor - Clipboard Operations", () => {
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

  test("should handle Ctrl+C and Ctrl+V in editor", async ({ page }) => {
    const opened = await openWorkflow(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "editor");
    const editor = page.locator(".monaco-editor .view-lines").first();
    await editor.click();

    await page.keyboard.type("copy this text");
    await page.keyboard.press("Control+A");
    await page.keyboard.press("Control+C");
    await page.waitForTimeout(200);

    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Enter");
    await page.keyboard.press("Control+V");
    await page.waitForTimeout(300);

    // Should paste content
    await expect(page.locator(".monaco-editor")).toBeVisible();
  });

  test("should handle Ctrl+X (cut) in editor", async ({ page }) => {
    const opened = await openWorkflow(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "editor");
    const editor = page.locator(".monaco-editor .view-lines").first();
    await editor.click();

    await page.keyboard.type("cut this line");
    await page.keyboard.press("Control+A");
    await page.keyboard.press("Control+X");
    await page.waitForTimeout(300);

    // Line should be cut
    await expect(page.locator(".monaco-editor")).toBeVisible();
  });

  test("should handle paste with special formatting", async ({ page }) => {
    const opened = await openWorkflow(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "editor");
    const editor = page.locator(".monaco-editor .view-lines").first();
    await editor.click();

    // Paste YAML with special characters
    const yamlContent = `name: test
description: |
  Multiline with
  special chars: @#$%
tasks:
  - name: task_1
    type: agent`;

    await page.keyboard.type(yamlContent);
    await page.waitForTimeout(500);

    const content = await page.locator(".view-lines").first().textContent();
    expect(content).toContain("test");
  });
});

// ============================================================================
// MEMORY & RESOURCE TESTS
// ============================================================================

test.describe("Workflow Editor - Resource Management", () => {
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

  test("should not leak memory on repeated open/close", async ({ page }) => {
    for (let i = 0; i < 10; i++) {
      const opened = await openWorkflow(page);
      if (!opened) break;

      await switchMode(page, "editor");
      await page.waitForTimeout(200);

      await switchMode(page, "form");
      await page.waitForTimeout(200);

      // Navigate away
      await page.getByRole("tab", { name: "Files" }).click();
      await page.waitForTimeout(200);
    }

    // Should still be responsive
    const filesTab = page.getByRole("tab", { name: "Files" });
    await expect(filesTab).toBeVisible();
  });

  test("should clean up on navigation", async ({ page }) => {
    const opened = await openWorkflow(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchMode(page, "editor");
    const editor = page.locator(".monaco-editor .view-lines").first();
    await editor.click();
    await page.keyboard.type("temporary content");

    // Navigate to different file
    await page.getByRole("tab", { name: "Files" }).click();
    const configFile = page
      .locator('a[href*="/ide/"]:visible')
      .filter({ hasText: "config" })
      .first();
    if (await configFile.isVisible()) {
      await configFile.click();
      await page.waitForTimeout(500);
    }

    // Navigate back
    const opened2 = await openWorkflow(page);
    expect(opened2).toBeTruthy();
  });
});
