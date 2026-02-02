import { test, expect } from "@playwright/test";
import { resetTestFile, resetTestAgentFile } from "./utils";
import { IDEPage } from "./pages/IDEPage";

test.describe("IDE Functionality", () => {
  test.beforeEach(async ({ page }) => {
    // Create/reset test files before each test so they're available in the IDE
    await resetTestFile();
    await resetTestAgentFile();
    await page.goto("/ide");
    await page.waitForLoadState("networkidle");

    // Wait for IDE sidebar tabs to be visible
    await expect(page.getByRole("tab", { name: "Files" })).toBeVisible({
      timeout: 10000,
    });

    // Switch to Files view mode (default is Objects mode)
    await page.getByRole("tab", { name: "Files" }).click();
    await page.waitForTimeout(500);
  });

  test("should display file browser with folders and files in Files mode", async ({
    page,
  }) => {
    const idePage = new IDEPage(page);

    // Verify we're in Files mode (switched in beforeEach)
    await idePage.verifyFilesMode();

    // Verify at least some folders are visible (folders that actually exist in test env)
    const workflowsFolder = page.getByRole("button", {
      name: "workflows",
      exact: true,
    });
    const generatedFolder = page.getByRole("button", {
      name: "generated",
      exact: true,
    });
    const exampleSqlFolder = page.getByRole("button", {
      name: "example_sql",
      exact: true,
    });

    // At least one folder should be visible
    const hasFolders =
      (await workflowsFolder.isVisible().catch(() => false)) ||
      (await generatedFolder.isVisible().catch(() => false)) ||
      (await exampleSqlFolder.isVisible().catch(() => false));

    expect(hasFolders).toBeTruthy();

    // Verify some files are visible
    await expect(page.getByRole("link", { name: "config.yml" })).toBeVisible();
  });

  test("should expand and collapse folders", async ({ page }) => {
    // Use workflows folder which exists in the test environment
    const folder = page.getByRole("button", { name: "workflows", exact: true });
    const fileInFolder = page.getByRole("link", {
      name: "fruit_sales_analyst.workflow.yml",
    });

    // Ensure folder starts in a known state (collapsed)
    // Check if file is visible, if so, close the folder first
    const isFileVisible = await fileInFolder.isVisible().catch(() => false);
    if (isFileVisible) {
      await folder.click();
      await expect(fileInFolder).not.toBeVisible();
    }

    // Now expand folder
    await folder.click();
    await expect(fileInFolder).toBeVisible();

    // Collapse folder
    await folder.click();
    await expect(fileInFolder).not.toBeVisible();
  });

  test("should open file and display in editor", async ({ page }) => {
    const idePage = new IDEPage(page);

    await idePage.openFile("config.yml");
    await idePage.verifyFileIsOpen("config.yml");
    await idePage.waitForEditorToLoad();
  });

  test("should display empty state when no file is open", async ({ page }) => {
    await expect(page.getByText("No file is open")).toBeVisible();
    await expect(
      page.getByText("Select a file from the sidebar to start editing"),
    ).toBeVisible();
  });

  test("should switch between Files and Objects modes", async ({ page }) => {
    const idePage = new IDEPage(page);

    // Verify we're in Files mode (switched in beforeEach)
    await idePage.verifyFilesMode();

    // Switch to Objects mode
    await idePage.switchToObjectsMode();
    await idePage.verifyObjectsMode();

    // Switch back to Files mode
    await idePage.switchToFilesMode();
    await idePage.verifyFilesMode();
  });

  test("should display objects grouped by type in Objects mode", async ({
    page,
  }) => {
    const idePage = new IDEPage(page);

    // Switch to Objects mode
    await idePage.switchToObjectsMode();

    // Verify we're in Objects mode (check the tab is active)
    await expect(page.getByRole("tab", { name: "Objects" })).toHaveAttribute(
      "data-state",
      "active",
    );

    // Check for at least one object group (the specific groups depend on what files exist)
    const agentsGroup = page.locator("text=Agents");
    const automationsGroup = page.locator("text=Automations");
    const semanticGroup = page.locator("text=Semantic Layer");
    const appsGroup = page.locator("text=Apps");

    // At least one group should be visible
    const hasVisibleGroup =
      (await agentsGroup.isVisible().catch(() => false)) ||
      (await automationsGroup.isVisible().catch(() => false)) ||
      (await semanticGroup.isVisible().catch(() => false)) ||
      (await appsGroup.isVisible().catch(() => false));

    expect(hasVisibleGroup).toBeTruthy();
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
      await idePage.openFile("config.yml");
      await idePage.verifyBreadcrumb("config.yml");

      // Open second file
      await idePage.openFile("test-file-for-e2e.txt");
      await idePage.verifyBreadcrumb("test-file-for-e2e.txt");

      // Open third file
      await idePage.openFile("semantics.yml");
      await idePage.verifyBreadcrumb("semantics.yml");
    });

    test("should handle typing special characters", async ({ page }) => {
      const idePage = new IDEPage(page);

      await idePage.openFile("test-file-for-e2e.txt");
      await idePage.insertTextAtEnd("Special chars: !@#$%^&*(){}[]<>");
      await idePage.verifySaveButtonVisible();
    });

    test("should edit YAML file", async ({ page }) => {
      const idePage = new IDEPage(page);

      // Open a YAML file that exists
      await idePage.openFile("config.yml");

      // Test edit and save functionality
      await idePage.insertTextAtEnd("# Test configuration line");
      await idePage.verifySaveButtonVisible();
      await idePage.saveFile();

      // Make another edit
      await idePage.insertTextAtEnd("\n# Another test line");
      await idePage.verifySaveButtonVisible();
      await idePage.saveFile();
    });
  });
});
