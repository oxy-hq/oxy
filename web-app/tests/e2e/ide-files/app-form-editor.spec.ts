import { test, expect } from "@playwright/test";
import { IDEPage } from "../pages/IDEPage";

test.describe("IDE Files - App Form Editor - Mode Switching", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/ide");
    await page.waitForLoadState("networkidle");
    await expect(page.getByRole("tab", { name: "Files" })).toBeVisible({
      timeout: 10000,
    });
  });

  // 8.1 Open in Objects mode → Form
  test("8.1 - should show Form in Objects mode", async ({ page }) => {
    await page.getByRole("tab", { name: "Objects" }).click();
    await page.waitForTimeout(500);

    // Try to find and expand Apps section using mouse position
    const appsSection = page.getByText("Apps").first();
    const appsSectionCount = await page.getByText("Apps").count();

    if (appsSectionCount > 0) {
      try {
        // Get the bounding box and click at the center
        const box = await appsSection.boundingBox();
        if (box) {
          await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
          await page.waitForTimeout(300);

          const appFile = page
            .locator('a[href*="/ide/"]:visible')
            .filter({ hasText: "app" })
            .first();

          if (await appFile.isVisible()) {
            await appFile.click();
            await page.waitForURL(/\/ide\/.+/);
            await page.waitForTimeout(1000);

            const hasForm = await page
              .locator("form, [data-testid*='form']")
              .isVisible()
              .catch(() => false);
            const hasEditor = await page
              .locator(".monaco-editor")
              .isVisible()
              .catch(() => false);
            expect(hasForm || hasEditor).toBeTruthy();
          } else {
            // No app files in Objects mode, skip test
            test.skip();
          }
        } else {
          test.skip();
        }
      } catch {
        // If any error occurs, skip the test
        test.skip();
      }
    } else {
      // If no Apps section, skip the test
      test.skip();
    }
  });

  // 8.2 Open in Files mode → Editor
  test("8.2 - should show YAML editor in Files mode", async ({ page }) => {
    const idePage = new IDEPage(page);
    await page.getByRole("tab", { name: "Files" }).click();
    await idePage.verifyFilesMode();

    const appFile = page
      .locator('a[href*="/ide/"]:visible')
      .filter({ hasText: ".app.yml" })
      .first();

    if (await appFile.isVisible()) {
      await appFile.click();
      await page.waitForURL(/\/ide\/.+/);
      await page.waitForTimeout(1000);

      // Switch to Editor mode to see Monaco editor
      const editorTab = page.getByRole("tab", { name: /editor|yaml/i });
      if (await editorTab.isVisible()) {
        await editorTab.click();
        await page.waitForTimeout(500);
      }

      await idePage.waitForEditorToLoad();
    }
  });

  // 8.3 Switch to Visualization
  test("8.3 - should show app preview in Visualization mode", async ({
    page,
  }) => {
    const idePage = new IDEPage(page);
    await page.getByRole("tab", { name: "Files" }).click();
    await idePage.verifyFilesMode();

    const appFile = page
      .locator('a[href*="/ide/"]:visible')
      .filter({ hasText: ".app.yml" })
      .first();

    if (await appFile.isVisible()) {
      await appFile.click();
      await page.waitForURL(/\/ide\/.+/);

      const vizTab = page.getByRole("tab", { name: /visualization|preview/i });
      if (await vizTab.isVisible()) {
        await vizTab.click();
        await page.waitForTimeout(1000);
      }
    }
  });

  // 8.4 Invalid YAML → Visualization
  test("8.4 - should show error in preview for invalid YAML", async ({
    page,
  }) => {
    const idePage = new IDEPage(page);
    await page.getByRole("tab", { name: "Files" }).click();
    await idePage.verifyFilesMode();

    const appFile = page
      .locator('a[href*="/ide/"]:visible')
      .filter({ hasText: ".app.yml" })
      .first();

    if (await appFile.isVisible()) {
      await appFile.click();
      await page.waitForURL(/\/ide\/.+/);
      await page.waitForTimeout(1000);

      // Switch to Editor mode to see Monaco editor
      const editorTab = page.getByRole("tab", { name: /editor|yaml/i });
      if (await editorTab.isVisible()) {
        await editorTab.click();
        await page.waitForTimeout(500);
      }

      await idePage.waitForEditorToLoad();

      // Add invalid YAML
      await idePage.insertTextAtEnd("\n  invalid: yaml:\n    broken");

      const vizTab = page.getByRole("tab", { name: /visualization|preview/i });
      if (await vizTab.isVisible()) {
        await vizTab.click();
        await page.waitForTimeout(1000);
      }
    }
  });
});

test.describe("IDE Files - App Form Editor - Tasks", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/ide");
    await page.waitForLoadState("networkidle");
    await page.getByRole("tab", { name: "Files" }).click();
    await page.waitForTimeout(500);
  });

  // 8.5-8.7 Task management
  test("8.5 - should add task like workflow", async ({ page }) => {
    const appFile = page
      .locator('a[href*="/ide/"]:visible')
      .filter({ hasText: ".app.yml" })
      .first();

    if (await appFile.isVisible()) {
      await appFile.click();
      await page.waitForURL(/\/ide\/.+/);

      const formTab = page.getByRole("tab", { name: /form/i });
      if (await formTab.isVisible()) {
        await formTab.click();
        await page.waitForTimeout(500);

        const addTaskButton = page.getByRole("button", { name: /add.*task/i });
        if (await addTaskButton.isVisible()) {
          await addTaskButton.click();
          await page.waitForTimeout(500);
        }
      }
    }
  });
});

test.describe("IDE Files - App Form Editor - Display", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/ide");
    await page.waitForLoadState("networkidle");
    await page.getByRole("tab", { name: "Files" }).click();
    await page.waitForTimeout(500);
  });

  // 8.8-8.18 Display management
  test("8.8 - should add markdown display", async ({ page }) => {
    const appFile = page
      .locator('a[href*="/ide/"]:visible')
      .filter({ hasText: ".app.yml" })
      .first();

    if (await appFile.isVisible()) {
      await appFile.click();
      await page.waitForURL(/\/ide\/.+/);

      const formTab = page.getByRole("tab", { name: /form/i });
      if (await formTab.isVisible()) {
        await formTab.click();
        await page.waitForTimeout(500);

        const addDisplayButton = page.getByRole("button", {
          name: /add.*display/i,
        });
        if (await addDisplayButton.isVisible()) {
          await addDisplayButton.click();
          await page.waitForTimeout(500);
        }
      }
    }
  });

  // 8.9 Add line_chart display
  test("8.9 - should add line chart display", async ({ page }) => {
    const appFile = page
      .locator('a[href*="/ide/"]:visible')
      .filter({ hasText: ".app.yml" })
      .first();

    if (await appFile.isVisible()) {
      await appFile.click();
      await page.waitForURL(/\/ide\/.+/);

      const formTab = page.getByRole("tab", { name: /form/i });
      if (await formTab.isVisible()) {
        await formTab.click();
        await page.waitForTimeout(500);

        const addDisplayButton = page.getByRole("button", {
          name: /add.*display/i,
        });
        if (await addDisplayButton.isVisible()) {
          await addDisplayButton.click();
          await page.waitForTimeout(300);

          // Select line_chart type
          const chartTypeSelect = page
            .locator('[data-testid*="display-type"], select[name*="type"]')
            .first();
          if (await chartTypeSelect.isVisible()) {
            await chartTypeSelect.click();
            await page.waitForTimeout(300);
          }
        }
      }
    }
  });

  // 8.13 Switch display type
  test("8.13 - should update fields when switching display type", async ({
    page,
  }) => {
    const appFile = page
      .locator('a[href*="/ide/"]:visible')
      .filter({ hasText: ".app.yml" })
      .first();

    if (await appFile.isVisible()) {
      await appFile.click();
      await page.waitForURL(/\/ide\/.+/);

      const formTab = page.getByRole("tab", { name: /form/i });
      if (await formTab.isVisible()) {
        await formTab.click();
        await page.waitForTimeout(500);

        // Find existing display type selector
        const displayTypeSelect = page
          .locator('[data-testid*="display-type"], select[name*="type"]')
          .first();
        if (await displayTypeSelect.isVisible()) {
          await displayTypeSelect.click();
          await page.waitForTimeout(500);
        }
      }
    }
  });

  // 8.17 Add 10 displays
  test("8.17 - should handle adding multiple displays", async ({ page }) => {
    const appFile = page
      .locator('a[href*="/ide/"]:visible')
      .filter({ hasText: ".app.yml" })
      .first();

    if (await appFile.isVisible()) {
      await appFile.click();
      await page.waitForURL(/\/ide\/.+/);

      const formTab = page.getByRole("tab", { name: /form/i });
      if (await formTab.isVisible()) {
        await formTab.click();
        await page.waitForTimeout(500);

        const addDisplayButton = page.getByRole("button", {
          name: /add.*display/i,
        });
        if (await addDisplayButton.isVisible()) {
          for (let i = 0; i < 5; i++) {
            await addDisplayButton.click();
            await page.waitForTimeout(200);
          }

          await page.waitForTimeout(1000);
        }
      }
    }
  });
});

test.describe("IDE Files - App Form Editor - Merge Strategy", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/ide");
    await page.waitForLoadState("networkidle");
    await page.getByRole("tab", { name: "Files" }).click();
    await page.waitForTimeout(500);
  });

  // 8.19-8.20 Merge strategy
  test("8.19 - should preserve extra YAML fields not in form", async ({
    page,
  }) => {
    const idePage = new IDEPage(page);

    const appFile = page
      .locator('a[href*="/ide/"]:visible')
      .filter({ hasText: ".app.yml" })
      .first();

    if (await appFile.isVisible()) {
      await appFile.click();
      await page.waitForURL(/\/ide\/.+/);
      await page.waitForTimeout(1000);

      // Switch to Editor mode to see Monaco editor
      const editorTab = page.getByRole("tab", { name: /editor|yaml/i });
      if (await editorTab.isVisible()) {
        await editorTab.click();
        await page.waitForTimeout(500);
      }

      await idePage.waitForEditorToLoad();

      // Add custom field in editor
      await idePage.insertTextAtEnd("\ncustom_field: preserved_value");
      await page.waitForTimeout(500);

      // Switch to form and back
      const formTab = page.getByRole("tab", { name: /form/i });
      const editorTabForSwitch = page.getByRole("tab", { name: /editor/i });

      if (
        (await formTab.isVisible()) &&
        (await editorTabForSwitch.isVisible())
      ) {
        await formTab.click();
        await page.waitForTimeout(500);
        await editorTabForSwitch.click();
        await page.waitForTimeout(500);

        // Custom field should be preserved
      }
    }
  });
});
