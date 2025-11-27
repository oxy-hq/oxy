import { test, expect } from "@playwright/test";
import { resetProject, resetTestFile } from "./utils";
import { IDEPage } from "./pages/IDEPage";

test.describe("IDE Functionality", () => {
  test.beforeEach(async ({ page }) => {
    resetProject();
    await page.goto("/ide");
  });

  // Clean up test file modifications after each test
  test.afterEach(async () => {
    await resetTestFile();
  });

  test("should display file browser with folders and files", async ({
    page,
  }) => {
    // Verify the Files header is visible
    await expect(page.getByText("Files").first()).toBeVisible();

    // Verify folders are visible (using folder buttons with exact match)
    await expect(
      page.getByRole("button", { name: "agents", exact: true }),
    ).toBeVisible();
    await expect(
      page.getByRole("button", { name: "workflows", exact: true }),
    ).toBeVisible();
    await expect(
      page.getByRole("button", { name: "apps", exact: true }),
    ).toBeVisible();

    // Verify some files are visible
    await expect(page.getByRole("link", { name: "README.md" })).toBeVisible();
    await expect(page.getByRole("link", { name: "config.yml" })).toBeVisible();
  });

  test("should expand and collapse folders", async ({ page }) => {
    const folder = page.getByRole("button", { name: "agents", exact: true });
    const fileInFolder = page.getByRole("link", { name: "duckdb.agent.yml" });

    // Ensure folder starts in a known state (collapsed)
    // Check if file is visible, if so, close the folder first
    const isFileVisible = await fileInFolder.isVisible().catch(() => false);
    if (isFileVisible) {
      await folder.click();
      await page.waitForTimeout(500);
    }

    // Now expand folder
    await folder.click();
    await page.waitForTimeout(1000);
    await expect(fileInFolder).toBeVisible({ timeout: 5000 });

    // Collapse folder
    await folder.click();
    await page.waitForTimeout(500);
    await expect(fileInFolder).not.toBeVisible({ timeout: 5000 });
  });

  test("should open file and display in editor", async ({ page }) => {
    const idePage = new IDEPage(page);

    await idePage.openFile("README.md");
    await idePage.verifyFileIsOpen("README.md");
    await idePage.waitForEditorToLoad();
  });

  test("should open file from nested folder", async ({ page }) => {
    const idePage = new IDEPage(page);

    await idePage.expandFolder("agents", "duckdb.agent.yml");
    await idePage.openFile("duckdb.agent.yml");
    await idePage.verifyFileIsOpen("duckdb.agent.yml");
  });

  test("should display empty state when no file is open", async ({ page }) => {
    await expect(page.getByText("No file is open")).toBeVisible();
    await expect(
      page.getByText("Select a file from the sidebar to start editing"),
    ).toBeVisible();
  });

  // Enhanced editing experience tests
  test.describe("Editing Experience", () => {
    test("should edit file and show save button", async ({ page }) => {
      const idePage = new IDEPage(page);

      await idePage.openFile("test-file-for-e2e.txt");
      await idePage.insertTextAtEnd("## Test Edit");
      await idePage.verifySaveButtonVisible();
    });

    test("should save file changes", async ({ page }) => {
      const idePage = new IDEPage(page);

      await idePage.openFile("test-file-for-e2e.txt");
      await idePage.insertTextAtEnd("## Saved Content");
      await idePage.saveFile();
      await idePage.verifySaveButtonHidden();
    });

    test("should add multiple lines to file", async ({ page }) => {
      const idePage = new IDEPage(page);

      await idePage.openFile("test-file-for-e2e.txt");
      await idePage.addMultipleLines([
        "## Section 1",
        "Content for section 1",
        "",
        "## Section 2",
        "Content for section 2",
      ]);
      await idePage.verifySaveButtonVisible();
    });

    test("should insert text at start of file", async ({ page }) => {
      const idePage = new IDEPage(page);

      await idePage.openFile("test-file-for-e2e.txt");
      await idePage.insertTextAtStart("# Header at top");
      await idePage.verifySaveButtonVisible();
    });

    test("should undo and redo changes", async ({ page }) => {
      const idePage = new IDEPage(page);

      await idePage.openFile("test-file-for-e2e.txt");

      // Make a change
      await idePage.insertTextAtEnd("## Test Content");
      await idePage.verifySaveButtonVisible();

      // Undo the change
      await idePage.undo();

      // Note: After undo, save button behavior depends on whether
      // the content matches the saved version. We'll just verify
      // undo executed without error

      // Redo the change
      await idePage.redo();
      await idePage.verifySaveButtonVisible();
    });

    test("should replace all content in file", async ({ page }) => {
      const idePage = new IDEPage(page);

      await idePage.openFile("test-file-for-e2e.txt");
      await idePage.replaceAllContent(
        "# Completely New Content\n\nThis replaces everything.",
      );
      await idePage.verifySaveButtonVisible();
    });

    test("should handle edit, save, edit workflow", async ({ page }) => {
      const idePage = new IDEPage(page);

      await idePage.openFile("test-file-for-e2e.txt");

      // First edit
      await idePage.insertTextAtEnd("## First Edit");
      await idePage.verifySaveButtonVisible();
      await idePage.saveFile();
      await idePage.verifySaveButtonHidden();

      // Second edit
      await idePage.insertTextAtEnd("## Second Edit");
      await idePage.verifySaveButtonVisible();
      await idePage.saveFile();
      await idePage.verifySaveButtonHidden();
    });

    test("should verify breadcrumb updates when switching files", async ({
      page,
    }) => {
      const idePage = new IDEPage(page);

      // Open first file
      await idePage.openFile("README.md");
      await idePage.verifyBreadcrumb("README.md");

      // Open second file
      await idePage.openFile("config.yml");
      await idePage.verifyBreadcrumb("config.yml");

      // Open file from nested folder
      await idePage.expandFolder("agents", "duckdb.agent.yml");
      await idePage.openFile("duckdb.agent.yml");
      await idePage.verifyBreadcrumb("agents/duckdb.agent.yml");
    });

    test("should handle typing special characters", async ({ page }) => {
      const idePage = new IDEPage(page);

      await idePage.openFile("test-file-for-e2e.txt");
      await idePage.insertTextAtEnd("Special chars: !@#$%^&*(){}[]<>");
      await idePage.verifySaveButtonVisible();
    });

    test("should edit YAML file in agents folder", async ({ page }) => {
      const idePage = new IDEPage(page);

      await idePage.expandFolder("agents", "duckdb.agent.yml");
      await idePage.openFile("duckdb.agent.yml");
      await idePage.insertTextAtEnd("  # New configuration line");
      await idePage.verifySaveButtonVisible();
      await idePage.saveFile();
    });
  });
});
