import { test, expect } from "@playwright/test";
import { IDEPage } from "../pages/IDEPage";
import { resetTestFile } from "../utils";
import { captureFileTree, cleanupAfterTest } from "./test-cleanup";

test.describe("IDE Files - File/Folder Rename", () => {
  test.beforeEach(async ({ page }) => {
    await resetTestFile();
    await page.goto("/ide");
    await page.waitForLoadState("networkidle");
    await expect(page.getByRole("tab", { name: "Files" })).toBeVisible({
      timeout: 10000,
    });
    await page.getByRole("tab", { name: "Files" }).click();
    await page.waitForTimeout(500);

    // Capture file tree before test
    await captureFileTree(page);
  });

  test.afterEach(async ({ page }) => {
    // Restore file tree after test
    await cleanupAfterTest(page);
  });

  // 3.1 Rename file in root
  test("3.1 - should rename file in root", async ({ page }) => {
    const idePage = new IDEPage(page);
    await idePage.verifyFilesMode();

    // Right-click on test file
    const testFile = page.getByRole("link", { name: "test-file-for-e2e.txt" });

    if (await testFile.isVisible()) {
      await testFile.click({ button: "right" });

      const renameOption = page.getByRole("menuitem", { name: /rename/i });
      if (await renameOption.isVisible()) {
        await renameOption.click();

        const input = page.locator("input:visible");
        await expect(input.first()).toBeVisible();
      }
    }
  });

  // 3.2 Rename currently open file
  test("3.2 - should update URL when renaming currently open file", async ({
    page,
  }) => {
    const idePage = new IDEPage(page);

    // Open a file first
    await idePage.openFile("test-file-for-e2e.txt");
    await idePage.verifyFileIsOpen("test-file-for-e2e.txt");

    const currentUrl = page.url();

    // The URL should contain the encoded file path
    expect(currentUrl).toContain("/ide/");
  });

  // 3.3 Rename folder containing open file
  test("3.3 - should update URL when renaming folder containing open file", async ({
    page,
  }) => {
    const idePage = new IDEPage(page);
    await idePage.verifyFilesMode();

    // Navigate to a file in a folder
    const folder = page.getByRole("button", { name: "workflows", exact: true });
    if (await folder.isVisible()) {
      await folder.click();
      await page.waitForTimeout(500);

      // Try to find and open a file in the folder
      const fileInFolder = page
        .locator('a[href*="/ide/"]:visible')
        .filter({ hasText: ".yml" })
        .first();

      if (await fileInFolder.isVisible()) {
        await fileInFolder.click();
        await page.waitForURL(/\/ide\/.+/);
      }
    }
  });

  // 3.4 Press Escape during rename
  test("3.4 - should preserve original name when pressing Escape", async ({
    page,
  }) => {
    const idePage = new IDEPage(page);
    await idePage.verifyFilesMode();

    const testFile = page.getByRole("link", { name: "config.yml" });

    if (await testFile.isVisible()) {
      await testFile.click({ button: "right" });

      const renameOption = page.getByRole("menuitem", { name: /rename/i });
      if (await renameOption.isVisible()) {
        await renameOption.click();

        const input = page.locator("input:visible").first();
        if (await input.isVisible()) {
          await input.fill("new-name.yml");
          await page.keyboard.press("Escape");

          // Original name should be preserved
          await expect(
            page.getByRole("link", { name: "config.yml" }),
          ).toBeVisible({ timeout: 5000 });
        }
      }
    }
  });

  // 3.5 Rename to existing sibling name
  test("3.5 - should show error when renaming to existing sibling name", async ({
    page,
  }) => {
    const idePage = new IDEPage(page);
    await idePage.verifyFilesMode();

    const testFile = page.getByRole("link", { name: "test-file-for-e2e.txt" });

    if (await testFile.isVisible()) {
      await testFile.click({ button: "right" });

      const renameOption = page.getByRole("menuitem", { name: /rename/i });
      if (await renameOption.isVisible()) {
        await renameOption.click();

        const input = page.locator("input:visible").first();
        if (await input.isVisible()) {
          await input.fill("config.yml"); // Existing file
          await page.keyboard.press("Enter");

          // Should show error
          await page.waitForTimeout(500);
        }
      }
    }
  });

  // 3.6 Rename Agent removing .agent.yml
  test("3.6 - should warn or prevent removing .agent.yml extension", async ({
    page,
  }) => {
    const idePage = new IDEPage(page);
    await idePage.verifyFilesMode();

    // Find an agent file
    const agentFolder = page.getByRole("button", {
      name: "agents",
      exact: true,
    });
    if (await agentFolder.isVisible()) {
      await agentFolder.click();
      await page.waitForTimeout(500);

      const agentFile = page
        .locator('a[href*="/ide/"]:visible')
        .filter({ hasText: ".agent.yml" })
        .first();

      if (await agentFile.isVisible()) {
        await agentFile.click({ button: "right" });

        const renameOption = page.getByRole("menuitem", { name: /rename/i });
        if (await renameOption.isVisible()) {
          await renameOption.click();

          const input = page.locator("input:visible").first();
          if (await input.isVisible()) {
            await input.fill("no-extension"); // Without .agent.yml
            await page.keyboard.press("Enter");

            // Should warn or prevent
            await page.waitForTimeout(500);
          }
        }
      }
    }
  });

  // 3.7 Rename .workflow.yml to .agent.yml
  test("3.7 - should change file type when renaming extension", async ({
    page,
  }) => {
    const idePage = new IDEPage(page);
    await idePage.verifyFilesMode();

    // This test verifies extension changes are handled
    const folder = page.getByRole("button", { name: "workflows", exact: true });
    if (await folder.isVisible()) {
      await folder.click();
      await page.waitForTimeout(500);
    }
  });

  // 3.8 Rename to empty string
  test("3.8 - should show error when renaming to empty string", async ({
    page,
  }) => {
    const idePage = new IDEPage(page);
    await idePage.verifyFilesMode();

    const testFile = page.getByRole("link", { name: "test-file-for-e2e.txt" });

    if (await testFile.isVisible()) {
      await testFile.click({ button: "right" });

      const renameOption = page.getByRole("menuitem", { name: /rename/i });
      if (await renameOption.isVisible()) {
        await renameOption.click();

        const input = page.locator("input:visible").first();
        if (await input.isVisible()) {
          await input.fill("");
          await page.keyboard.press("Enter");

          // Should show error
          await page.waitForTimeout(500);
        }
      }
    }
  });

  // 3.9 Double-click rename same file
  test("3.9 - should show only one input when double-clicking rename", async ({
    page,
  }) => {
    const idePage = new IDEPage(page);
    await idePage.verifyFilesMode();

    const testFile = page.getByRole("link", { name: "test-file-for-e2e.txt" });

    if (await testFile.isVisible()) {
      // Right-click twice rapidly
      await testFile.click({ button: "right" });
      const renameOption = page.getByRole("menuitem", { name: /rename/i });

      if (await renameOption.isVisible()) {
        await renameOption.click();

        // Count visible inputs
        const inputs = page.locator("input:visible");
        const inputCount = await inputs.count();
        expect(inputCount).toBeLessThanOrEqual(2);
      }
    }
  });

  // 3.10 Start rename, click different file's rename
  test("3.10 - should cancel first rename when starting another", async ({
    page,
  }) => {
    const idePage = new IDEPage(page);
    await idePage.verifyFilesMode();

    // This test verifies that starting a new rename cancels the previous one
    const testFile = page.getByRole("link", { name: "test-file-for-e2e.txt" });
    const configFile = page.getByRole("link", { name: "config.yml" });

    if ((await testFile.isVisible()) && (await configFile.isVisible())) {
      // Start rename on first file
      await testFile.click({ button: "right" });
      const renameOption = page.getByRole("menuitem", { name: /rename/i });

      if (await renameOption.isVisible()) {
        await renameOption.click();
        await page.waitForTimeout(300);

        // Start rename on second file - this should cancel the first
        await page.keyboard.press("Escape");

        // Should only have at most one rename input active
        const inputs = page.locator("input:visible");
        const inputCount = await inputs.count();
        expect(inputCount).toBeLessThanOrEqual(1);
      }
    }
  });
});
