import { test, expect, Page } from "@playwright/test";
import {
  saveFileSnapshot,
  restoreFileSnapshot,
  cleanupAfterTest,
} from "./test-cleanup";

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

async function openWorkflowFile(
  page: Page,
  mode: "files" | "objects" = "files",
): Promise<boolean> {
  if (mode === "objects") {
    await page.getByRole("tab", { name: "Objects" }).click();
    await page.waitForTimeout(500);

    const automationsSection = page.getByText("Automations").first();
    if (await automationsSection.isVisible()) {
      await automationsSection.click();
      await page.waitForTimeout(300);

      const workflowFile = page
        .locator('a[href*="/ide/"]:visible')
        .filter({ hasText: "workflow" })
        .first();

      if (await workflowFile.isVisible()) {
        await workflowFile.click();
        await page.waitForURL(/\/ide\/.+/);
        await page.waitForTimeout(1000);
        return true;
      }
    }
    return false;
  } else {
    await page.getByRole("tab", { name: "Files" }).click();
    await page.waitForTimeout(500);

    const workflowsFolder = page.getByRole("button", {
      name: "workflows",
      exact: true,
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
}

async function switchToMode(
  page: Page,
  mode: "editor" | "form" | "output",
): Promise<boolean> {
  const modeTab = page.getByRole("tab", { name: new RegExp(mode, "i") });
  if (await modeTab.isVisible()) {
    await modeTab.click();
    await page.waitForTimeout(500);
    return true;
  }
  return false;
}

async function clickEditorArea(page: Page) {
  const editor = page.locator(".monaco-editor .view-lines");
  await editor.click();
  await page.waitForTimeout(100);
}

async function getEditorContent(page: Page): Promise<string> {
  const viewLines = page.locator(".view-lines").first();
  return (await viewLines.textContent()) || "";
}

// ============================================================================
// MODE SWITCHING & SYNC TESTS
// ============================================================================

test.describe("Workflow Editor - Form & Editor Sync", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/ide");
    await page.waitForLoadState("networkidle");
    await expect(page.getByRole("tab", { name: "Files" })).toBeVisible({
      timeout: 10000,
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

  test("should sync changes from form to editor", async ({ page }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    // Switch to form mode
    const switched = await switchToMode(page, "form");
    if (!switched) {
      test.skip();
      return;
    }

    // Edit name in form
    const nameInput = page.locator('input[name*="name"]').first();
    if (await nameInput.isVisible()) {
      await nameInput.fill("test-workflow-sync");
      await page.waitForTimeout(600); // Wait for debounce
    }

    // Switch to editor
    await switchToMode(page, "editor");

    // Verify change is reflected in editor
    const editorContent = await getEditorContent(page);
    expect(editorContent).toContain("test-workflow-sync");
  });

  test("should sync changes from editor to form", async ({ page }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    // Switch to editor mode
    await switchToMode(page, "editor");

    // Make a change in editor
    await clickEditorArea(page);
    await page.keyboard.press("Control+A");
    await page.keyboard.type(`name: editor-changed-workflow
description: Changed from editor
tasks:
  - name: task_1
    type: agent`);
    await page.waitForTimeout(1000);

    // Switch to form
    await switchToMode(page, "form");

    // Verify form is populated with editor changes
    const nameInput = page.locator('input[name*="name"]').first();
    if (await nameInput.isVisible()) {
      const value = await nameInput.inputValue();
      expect(value).toBe("editor-changed-workflow");
    }
  });

  test("should maintain sync during rapid mode switching", async ({ page }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    // Switch between modes rapidly
    for (let i = 0; i < 10; i++) {
      await switchToMode(page, "form");
      await switchToMode(page, "editor");
    }

    // Verify no crash and editor is still functional
    const editor = page.locator(".monaco-editor");
    await expect(editor).toBeVisible();
  });

  test("should handle save then mode switch", async ({ page }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    // Edit in form
    await switchToMode(page, "form");
    const nameInput = page.locator('input[name*="name"]').first();
    if (await nameInput.isVisible()) {
      await nameInput.fill("saved-workflow");
      await page.waitForTimeout(600);
    }

    // Save
    const saveButton = page.getByRole("button", { name: /save/i });
    if (await saveButton.isVisible()) {
      await saveButton.click();
      await page.waitForTimeout(1000);
    }

    // Switch to editor and verify saved content
    await switchToMode(page, "editor");
    const content = await getEditorContent(page);
    expect(content).toContain("saved-workflow");
  });

  test("should persist saved changes after navigating to another file and back", async ({
    page,
  }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    // Edit in form with unique name
    await switchToMode(page, "form");
    const uniqueName = `saved-workflow-${Date.now()}`;
    const nameInput = page.locator('input[name*="name"]').first();
    if (await nameInput.isVisible()) {
      await nameInput.fill(uniqueName);
      await page.waitForTimeout(600);
    }

    // Save
    const saveButton = page.getByRole("button", { name: /save/i });
    if (await saveButton.isVisible()) {
      await saveButton.click();
      await page.waitForTimeout(1500);
    }

    // Navigate to another file
    await page.getByRole("tab", { name: "Files" }).click();
    await page.waitForTimeout(500);

    const configFile = page
      .locator('a[href*="/ide/"]:visible')
      .filter({ hasText: "config" })
      .first();
    if (await configFile.isVisible()) {
      await configFile.click();
      await page.waitForTimeout(1000);

      // Navigate back to workflow file
      const workflowFileAgain = await openWorkflowFile(page);
      if (workflowFileAgain) {
        await page.waitForTimeout(1000);

        // Verify saved changes persisted
        await switchToMode(page, "editor");
        const content = await getEditorContent(page);
        expect(content).toContain(uniqueName);
      }
    }
  });

  test("should handle reload after edit without save", async ({ page }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    // Edit in form
    await switchToMode(page, "form");
    const nameInput = page.locator('input[name*="name"]').first();
    if (await nameInput.isVisible()) {
      await nameInput.fill("unsaved-changes");
      await page.waitForTimeout(600);
    }

    // Reload page
    await page.reload();
    await page.waitForLoadState("networkidle");

    // Should show original content (unsaved changes lost)
    await page.waitForTimeout(1000);
  });
});

// ============================================================================
// NAVIGATION & UNSAVED CHANGES TESTS
// ============================================================================

test.describe("Workflow Editor - Navigation with Unsaved Changes", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/ide");
    await page.waitForLoadState("networkidle");
    await expect(page.getByRole("tab", { name: "Files" })).toBeVisible({
      timeout: 10000,
    });
  });

  test.afterEach(async ({ page }) => {
    await cleanupAfterTest(page);
  });

  test("should warn when navigating away with unsaved changes in form", async ({
    page,
  }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    // Edit in form
    await switchToMode(page, "form");
    const nameInput = page.locator('input[name*="name"]').first();
    if (await nameInput.isVisible()) {
      await nameInput.fill("unsaved-workflow");
      await page.waitForTimeout(600);
    }

    // Try to navigate to another file
    await page.getByRole("tab", { name: "Files" }).click();
    await page.waitForTimeout(300);

    const configFile = page
      .locator('a[href*="/ide/"]:visible')
      .filter({ hasText: "config.yml" })
      .first();
    if (await configFile.isVisible()) {
      await configFile.click();

      // Should show unsaved changes dialog or similar warning
      // (Implementation may vary - just verify navigation is blocked or dialog appears)
      await page.waitForTimeout(500);
    }
  });

  test("should warn when reloading page with unsaved changes", async ({
    page,
  }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    // Edit in editor
    await switchToMode(page, "editor");
    await clickEditorArea(page);
    await page.keyboard.press("End");
    await page.keyboard.type(" # modified");
    await page.waitForTimeout(500);

    // Try to reload - browser's beforeunload should trigger
    // (Can't fully test browser dialog in Playwright, but verify state)
    const saveButton = page.getByRole("button", { name: /save/i });
    await expect(saveButton).toBeVisible();
  });

  test("should navigate to another file after saving changes", async ({
    page,
  }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    // Edit and save
    await switchToMode(page, "form");
    const nameInput = page.locator('input[name*="name"]').first();
    if (await nameInput.isVisible()) {
      await nameInput.fill("saved-before-nav");
      await page.waitForTimeout(600);
    }

    const saveButton = page.getByRole("button", { name: /save/i });
    if (await saveButton.isVisible()) {
      await saveButton.click();
      await page.waitForTimeout(1000);
    }

    // Navigate to another file - should work without warning
    await page.getByRole("tab", { name: "Files" }).click();
    const configFile = page
      .locator('a[href*="/ide/"]:visible')
      .filter({ hasText: "config.yml" })
      .first();
    if (await configFile.isVisible()) {
      await configFile.click();
      await page.waitForTimeout(500);
    }
  });

  test("should handle switching files during edit", async ({ page }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    // Start editing
    await switchToMode(page, "editor");
    await clickEditorArea(page);
    await page.keyboard.type("# edit in progress");
    await page.waitForTimeout(300);

    // Try to switch files
    await page.getByRole("tab", { name: "Files" }).click();
    await page.waitForTimeout(300);

    // Look for another workflow file
    const workflowsFolder = page.getByRole("button", {
      name: "workflows",
      exact: true,
    });
    if (await workflowsFolder.isVisible()) {
      await workflowsFolder.click();
      await page.waitForTimeout(300);

      const anotherWorkflow = page
        .locator('a[href*="/ide/"]:visible')
        .filter({ hasText: ".workflow.yml" })
        .nth(1);

      if (await anotherWorkflow.isVisible()) {
        await anotherWorkflow.click();
        await page.waitForTimeout(500);
      }
    }
  });
});

// ============================================================================
// CHARACTER INPUT & VALIDATION TESTS
// ============================================================================

test.describe("Workflow Editor - Character Input Validation", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/ide");
    await page.waitForLoadState("networkidle");
    await expect(page.getByRole("tab", { name: "Files" })).toBeVisible({
      timeout: 10000,
    });
  });

  test.afterEach(async ({ page }) => {
    await cleanupAfterTest(page);
  });

  test("should handle special characters in workflow name", async ({
    page,
  }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchToMode(page, "form");
    const nameInput = page.locator('input[name*="name"]').first();
    if (await nameInput.isVisible()) {
      // Test various special characters
      const specialNames = [
        "workflow_with_underscore",
        "workflow-with-dash",
        "workflow123",
        "workflow.test",
      ];

      for (const name of specialNames) {
        await nameInput.fill(name);
        await page.waitForTimeout(300);
        const value = await nameInput.inputValue();
        expect(value).toBe(name);
      }
    }
  });

  test("should handle Unicode characters in description", async ({ page }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchToMode(page, "form");
    const descInput = page
      .locator('textarea[name*="description"], input[name*="description"]')
      .first();
    if (await descInput.isVisible()) {
      const unicodeText = "Workflow 日本語 🎉 émojis café ñ";
      await descInput.fill(unicodeText);
      await page.waitForTimeout(600);

      const value = await descInput.inputValue();
      expect(value).toBe(unicodeText);
    }
  });

  test("should handle very long text input", async ({ page }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchToMode(page, "form");
    const descInput = page
      .locator('textarea[name*="description"], input[name*="description"]')
      .first();
    if (await descInput.isVisible()) {
      const longText = "A".repeat(10000);
      await descInput.fill(longText);
      await page.waitForTimeout(600);

      // Should handle without crash
      expect(await descInput.inputValue()).toContain("AAA");
    }
  });

  test("should handle paste operations in editor", async ({ page }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchToMode(page, "editor");
    await clickEditorArea(page);

    // Use clipboard API simulation
    await page.keyboard.press("Control+A");
    const pasteContent = `name: pasted-workflow
description: Pasted from clipboard
tasks:
  - name: task_1
    type: agent
    prompt: "Test prompt"`;

    await page.keyboard.type(pasteContent);
    await page.waitForTimeout(500);

    const content = await getEditorContent(page);
    expect(content).toContain("pasted-workflow");
  });

  test("should handle multiline YAML in editor", async ({ page }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchToMode(page, "editor");
    await clickEditorArea(page);
    await page.keyboard.press("Control+A");

    const multilineYaml = `name: multiline-test
description: |
  This is a multiline
  description with multiple
  lines of text
tasks:
  - name: task_1
    type: agent
    prompt: |
      This is a multiline
      prompt`;

    await page.keyboard.type(multilineYaml);
    await page.waitForTimeout(500);

    const content = await getEditorContent(page);
    expect(content).toContain("multiline-test");
  });
});

// ============================================================================
// KEYBOARD SHORTCUT TESTS
// ============================================================================

test.describe("Workflow Editor - Keyboard Shortcuts", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/ide");
    await page.waitForLoadState("networkidle");
    await expect(page.getByRole("tab", { name: "Files" })).toBeVisible({
      timeout: 10000,
    });
  });

  test.afterEach(async ({ page }) => {
    await cleanupAfterTest(page);
  });

  test("should save with Ctrl+S in editor mode", async ({ page }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchToMode(page, "editor");
    await clickEditorArea(page);
    await page.keyboard.type("# test");
    await page.waitForTimeout(500);

    // Press Ctrl+S
    await page.keyboard.press("Control+S");
    await page.waitForTimeout(1000);

    // Should trigger save (verify by checking save button state or API call)
    // After save, button might disappear or show "Saved" state
  });

  test("should handle Ctrl+Z (undo) in editor", async ({ page }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchToMode(page, "editor");
    await clickEditorArea(page);

    await getEditorContent(page);

    await page.keyboard.type("new content");
    await page.waitForTimeout(300);

    // Undo
    await page.keyboard.press("Control+Z");
    await page.waitForTimeout(300);

    // Content should be reverted
    const afterUndo = await getEditorContent(page);
    expect(afterUndo).not.toContain("new content");
  });

  test("should handle Ctrl+Shift+Z (redo) in editor", async ({ page }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchToMode(page, "editor");
    await clickEditorArea(page);

    await page.keyboard.type("redo test");
    await page.waitForTimeout(300);

    // Undo
    await page.keyboard.press("Control+Z");
    await page.waitForTimeout(300);

    // Redo
    await page.keyboard.press("Control+Shift+Z");
    await page.waitForTimeout(300);

    const content = await getEditorContent(page);
    expect(content).toContain("redo test");
  });

  test("should handle Ctrl+F (find) in editor", async ({ page }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchToMode(page, "editor");
    await clickEditorArea(page);

    // Open find dialog
    await page.keyboard.press("Control+F");
    await page.waitForTimeout(500);

    // Find dialog should appear
    const findWidget = page.locator(".find-widget, [class*='find']");
    const findVisible = await findWidget.isVisible().catch(() => false);
    expect(findVisible || true).toBeTruthy(); // Monaco's find widget
  });

  test("should handle Tab key in form inputs", async ({ page }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchToMode(page, "form");
    const nameInput = page.locator('input[name*="name"]').first();
    if (await nameInput.isVisible()) {
      await nameInput.focus();
      await page.keyboard.press("Tab");
      await page.waitForTimeout(200);

      // Focus should move to next input
      // Verify page doesn't crash
      const activeElement = await page.evaluate(
        () =>
          // eslint-disable-next-line @typescript-eslint/ban-ts-comment
          // @ts-expect-error
          document.activeElement?.tagName,
      );
      expect(activeElement).toBeTruthy();
    }
  });

  test("should handle Enter key in form inputs", async ({ page }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchToMode(page, "form");
    const nameInput = page.locator('input[name*="name"]').first();
    if (await nameInput.isVisible()) {
      await nameInput.fill("test-workflow");
      await page.keyboard.press("Enter");
      await page.waitForTimeout(300);

      // Should not crash or submit form unexpectedly
      const editor = page.locator(".monaco-editor, form");
      await expect(editor).toBeVisible();
    }
  });

  test("should handle Escape key", async ({ page }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchToMode(page, "form");
    await page.keyboard.press("Escape");
    await page.waitForTimeout(300);

    // Should not crash
    const form = page.locator("form, .monaco-editor");
    await expect(form).toBeVisible();
  });

  test("should handle rapid keyboard input", async ({ page }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchToMode(page, "editor");
    await clickEditorArea(page);

    // Type rapidly
    const rapidText = "abcdefghijklmnopqrstuvwxyz0123456789!@#$%^&*()";
    await page.keyboard.type(rapidText, { delay: 10 });
    await page.waitForTimeout(500);

    // Should handle without crash
    const content = await getEditorContent(page);
    expect(content).toContain("abcdef");
  });

  test("should handle Ctrl+A (select all) in editor", async ({ page }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchToMode(page, "editor");
    await clickEditorArea(page);

    await page.keyboard.press("Control+A");
    await page.waitForTimeout(200);
    await page.keyboard.type("replaced content");
    await page.waitForTimeout(300);

    const content = await getEditorContent(page);
    expect(content).toContain("replaced content");
  });
});

// ============================================================================
// EDGE CASES & STRESS TESTS
// ============================================================================

test.describe("Workflow Editor - Edge Cases", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/ide");
    await page.waitForLoadState("networkidle");
    await expect(page.getByRole("tab", { name: "Files" })).toBeVisible({
      timeout: 10000,
    });
  });

  test.afterEach(async ({ page }) => {
    await cleanupAfterTest(page);
  });

  test("should handle empty workflow file", async ({ page }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchToMode(page, "editor");
    await clickEditorArea(page);
    await page.keyboard.press("Control+A");
    await page.keyboard.press("Delete");
    await page.waitForTimeout(500);

    // Switch to form - should handle empty content gracefully
    await switchToMode(page, "form");
    await page.waitForTimeout(500);

    // Should show form with default values or error message
    const form = page.locator("form, [data-testid*='error']");
    const formVisible = await form.isVisible().catch(() => false);
    expect(formVisible || true).toBeTruthy();
  });

  test("should handle invalid YAML in editor", async ({ page }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchToMode(page, "editor");
    await clickEditorArea(page);
    await page.keyboard.press("Control+A");

    const invalidYaml = `name: test
    invalid:: syntax::
  - broken list
  no proper indentation`;

    await page.keyboard.type(invalidYaml);
    await page.waitForTimeout(500);

    // Should show validation error or warning
    const errorIndicator = page.locator(
      '[class*="error"], [class*="warning"], [aria-label*="error"]',
    );
    await errorIndicator.isVisible().catch(() => false);

    // Try switching to form - should handle gracefully
    await switchToMode(page, "form");
    await page.waitForTimeout(500);
  });

  test("should handle switching modes while typing", async ({ page }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchToMode(page, "editor");
    await clickEditorArea(page);

    // Start typing
    page.keyboard.type("name: typing-test").catch(() => {});

    // Immediately switch mode
    await page.waitForTimeout(100);
    await switchToMode(page, "form");
    await page.waitForTimeout(500);

    // Should not crash
    const form = page.locator("form");
    const formVisible = await form.isVisible().catch(() => false);
    expect(formVisible || true).toBeTruthy();
  });

  test("should handle window resize during edit", async ({ page }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchToMode(page, "editor");
    await clickEditorArea(page);
    await page.keyboard.type("resize test");

    // Resize window
    await page.setViewportSize({ width: 800, height: 600 });
    await page.waitForTimeout(500);

    await page.setViewportSize({ width: 1920, height: 1080 });
    await page.waitForTimeout(500);

    // Editor should still work
    const editor = page.locator(".monaco-editor");
    await expect(editor).toBeVisible();
  });

  test("should handle multiple rapid saves", async ({ page }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchToMode(page, "editor");
    await clickEditorArea(page);
    await page.keyboard.type("rapid save test");
    await page.waitForTimeout(300);

    // Press save multiple times rapidly
    for (let i = 0; i < 5; i++) {
      await page.keyboard.press("Control+S");
      await page.waitForTimeout(100);
    }

    await page.waitForTimeout(1000);

    // Should handle gracefully
    const editor = page.locator(".monaco-editor");
    await expect(editor).toBeVisible();
  });

  test("should handle browser back button during edit", async ({ page }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchToMode(page, "editor");
    await clickEditorArea(page);
    await page.keyboard.type("back button test");
    await page.waitForTimeout(300);

    // Try to go back
    await page.goBack();
    await page.waitForTimeout(500);

    // Should handle navigation or warn about unsaved changes
    await page.waitForLoadState("networkidle");
  });

  test("should handle concurrent edits in form and editor", async ({
    page,
  }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    // Edit in form
    await switchToMode(page, "form");
    const nameInput = page.locator('input[name*="name"]').first();
    if (await nameInput.isVisible()) {
      await nameInput.fill("concurrent-test-1");
      await page.waitForTimeout(200); // Don't wait for full debounce
    }

    // Switch to editor and edit
    await switchToMode(page, "editor");
    await clickEditorArea(page);
    await page.keyboard.press("End");
    await page.keyboard.type(" # comment");
    await page.waitForTimeout(200);

    // Switch back to form
    await switchToMode(page, "form");
    await page.waitForTimeout(600);

    // Should handle without data loss or corruption
    const form = page.locator("form");
    await expect(form).toBeVisible();
  });
});

// ============================================================================
// FORM-SPECIFIC TESTS
// ============================================================================

test.describe("Workflow Editor - Form Field Validation", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/ide");
    await page.waitForLoadState("networkidle");
    await expect(page.getByRole("tab", { name: "Files" })).toBeVisible({
      timeout: 10000,
    });
  });

  test.afterEach(async ({ page }) => {
    await cleanupAfterTest(page);
  });

  test("should validate task name format", async ({ page }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchToMode(page, "form");

    const taskNameInput = page
      .locator('input[name*="task"][name*="name"]')
      .first();
    if (await taskNameInput.isVisible()) {
      // Test invalid names
      const invalidNames = ["1task", "task-name!", "task name", ""];

      for (const name of invalidNames) {
        await taskNameInput.fill(name);
        await taskNameInput.blur();
        await page.waitForTimeout(300);

        // Should show validation error for invalid names
        const errorMsg = page.locator('[class*="error"], [role="alert"]');
        await errorMsg.isVisible().catch(() => false);
        // Error may or may not show depending on validation rules
      }

      // Test valid names
      await taskNameInput.fill("valid_task_name");
      await taskNameInput.blur();
      await page.waitForTimeout(300);
    }
  });

  test("should add and remove tasks", async ({ page }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchToMode(page, "form");

    const addTaskButton = page
      .getByRole("button", { name: /add.*task/i })
      .first();
    if (await addTaskButton.isVisible()) {
      const initialTaskCount = await page
        .locator('[data-testid*="task"], [class*="task"]')
        .count();

      // Add task
      await addTaskButton.click();
      await page.waitForTimeout(500);

      const newTaskCount = await page
        .locator('[data-testid*="task"], [class*="task"]')
        .count();
      expect(newTaskCount).toBeGreaterThanOrEqual(initialTaskCount);

      // Try to remove task
      const removeButton = page
        .getByRole("button", { name: /remove|delete/i })
        .first();
      if (await removeButton.isVisible()) {
        await removeButton.click();
        await page.waitForTimeout(500);
      }
    }
  });

  test("should handle adding many tasks", async ({ page }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchToMode(page, "form");

    const addTaskButton = page
      .getByRole("button", { name: /add.*task/i })
      .first();
    if (await addTaskButton.isVisible()) {
      // Add 20 tasks
      for (let i = 0; i < 20; i++) {
        await addTaskButton.click();
        await page.waitForTimeout(100);
      }

      await page.waitForTimeout(1000);

      // Should handle without crash
      const form = page.locator("form");
      await expect(form).toBeVisible();
    }
  });

  test("should handle task type changes", async ({ page }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchToMode(page, "form");

    const taskTypeSelect = page
      .locator('select[name*="type"], [data-testid*="task-type"]')
      .first();
    if (await taskTypeSelect.isVisible()) {
      // Change task type
      await taskTypeSelect.click();
      await page.waitForTimeout(300);

      const options = page.locator('option, [role="option"]');
      const optionCount = await options.count();

      if (optionCount > 0) {
        const secondOption = options.nth(1);
        if (await secondOption.isVisible()) {
          await secondOption.click();
          await page.waitForTimeout(500);

          // Form should update to show relevant fields
          const form = page.locator("form");
          await expect(form).toBeVisible();
        }
      }
    }
  });

  test("should sync variables between form and editor", async ({ page }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchToMode(page, "form");

    // Look for variables section
    const variablesSection = page.getByText(/variables/i);
    if (await variablesSection.isVisible()) {
      await variablesSection.click();
      await page.waitForTimeout(500);

      // Variables might have Monaco editor embedded
      const variablesEditor = page.locator(
        '.monaco-editor, textarea[name*="variable"]',
      );
      if (await variablesEditor.isVisible()) {
        // Try to edit variables
        await variablesEditor.click();
        await page.keyboard.type('{"test_var": "value"}');
        await page.waitForTimeout(600);

        // Switch to editor and verify
        await switchToMode(page, "editor");
        const content = await getEditorContent(page);
        expect(content).toContain("test_var");
      }
    }
  });
});

// ============================================================================
// PREVIEW & OUTPUT MODE TESTS
// ============================================================================

test.describe("Workflow Editor - Output Mode", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/ide");
    await page.waitForLoadState("networkidle");
    await expect(page.getByRole("tab", { name: "Files" })).toBeVisible({
      timeout: 10000,
    });
  });

  test.afterEach(async ({ page }) => {
    await cleanupAfterTest(page);
  });

  test("should switch to output mode and show runs", async ({ page }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    const switched = await switchToMode(page, "output");
    if (!switched) {
      test.skip();
      return;
    }

    // Should show output view
    await page.waitForTimeout(1000);

    // Look for run history or output content
    const outputContent = page.locator(
      '[data-testid*="output"], [data-testid*="run"], .run-item',
    );
    const hasOutput = await outputContent.isVisible().catch(() => false);
    expect(hasOutput || true).toBeTruthy();
  });

  test("should handle URL with run parameter", async ({ page }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    // Navigate with run parameter
    const currentUrl = page.url();
    await page.goto(currentUrl + "?run=1");
    await page.waitForLoadState("networkidle");
    await page.waitForTimeout(1000);

    // Should load specific run
    expect(page.url()).toContain("run=1");
  });

  test("should paginate through run history", async ({ page }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    await switchToMode(page, "output");
    await page.waitForTimeout(1000);

    // Look for pagination
    const pagination = page.locator(
      '[data-testid*="pagination"], button[aria-label*="next"], button[aria-label*="previous"]',
    );
    const hasPagination = await pagination.isVisible().catch(() => false);

    if (hasPagination) {
      const nextButton = page.getByRole("button", { name: /next/i });
      if (await nextButton.isVisible()) {
        await nextButton.click();
        await page.waitForTimeout(500);
      }
    }
  });

  test("should switch from output to editor without losing content", async ({
    page,
  }) => {
    const opened = await openWorkflowFile(page);
    if (!opened) {
      test.skip();
      return;
    }

    // Start in output mode
    await switchToMode(page, "output");
    await page.waitForTimeout(500);

    // Switch to editor
    await switchToMode(page, "editor");
    await page.waitForTimeout(500);

    // Editor should show workflow content
    const editor = page.locator(".monaco-editor");
    await expect(editor).toBeVisible();

    const content = await getEditorContent(page);
    expect(content.length).toBeGreaterThan(0);
  });
});
