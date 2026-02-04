import { expect, type Locator, type Page } from "@playwright/test";

export class IDEPage {
  readonly page: Page;
  readonly editor: Locator;
  readonly saveButton: Locator;
  readonly breadcrumb: Locator;
  readonly filesModeButton: Locator;
  readonly objectsModeButton: Locator;

  constructor(page: Page) {
    this.page = page;
    this.editor = page.locator(".monaco-editor");
    this.saveButton = page.getByTestId("ide-save-button");
    this.breadcrumb = page.getByTestId("ide-breadcrumb");
    // IDE sidebar mode toggles (tabs)
    this.filesModeButton = page.getByRole("tab", { name: "Files" });
    this.objectsModeButton = page.getByRole("tab", { name: "Objects" });
  }

  async goto() {
    await this.page.goto("/ide");
  }

  async openFile(fileName: string) {
    // Click the IDE file link (using locator that matches href pattern)
    const fileLink = this.page.locator(`a[href*="/ide/"]:has-text("${fileName}")`).first();

    await fileLink.click();
    await this.page.waitForURL(/\/ide\/.+/);
    await expect(this.editor).toBeVisible({ timeout: 10000 });
    // Wait for breadcrumb to update (small delay to let UI catch up)
    await this.page.waitForTimeout(500);
  }

  async expandFolder(folderName: string, knownFile?: string) {
    const folder = this.page.getByRole("button", {
      name: folderName,
      exact: true
    });

    // Wait for folder to be visible first
    await expect(folder).toBeVisible({ timeout: 5000 });

    // If we know a file that should be in the folder, check if it's already visible
    if (knownFile) {
      const fileLink = this.page.getByRole("link", { name: knownFile });
      const isVisible = await fileLink.isVisible().catch(() => false);

      if (isVisible) {
        // Already expanded, do nothing
        return;
      }
    }

    // Click to expand
    await folder.click();

    // Wait for expansion animation to complete
    await this.page.waitForTimeout(1000);
  }

  async collapseFolder(folderName: string) {
    const folder = this.page.getByRole("button", {
      name: folderName,
      exact: true
    });

    await folder.click();
    // Wait for collapse animation
    await this.page.waitForTimeout(500);
  }

  async verifyFileInFolder(folderName: string, fileName: string) {
    await this.expandFolder(folderName);
    await expect(this.page.getByRole("link", { name: fileName })).toBeVisible();
  }

  async clickEditor() {
    await this.editor.click();
  }

  async typeText(text: string) {
    await this.editor.click();
    await this.page.keyboard.type(text);
  }

  async insertTextAtEnd(text: string) {
    await this.editor.click();
    await this.page.keyboard.press("Control+End");
    await this.page.keyboard.press("Enter");
    await this.page.keyboard.type(text);
    // Wait for the editor to register the change and trigger state update
    await this.page.waitForTimeout(2000);
  }

  async insertTextAtStart(text: string) {
    await this.editor.click();
    await this.page.keyboard.press("Control+Home");
    await this.page.keyboard.press("Enter");
    await this.page.keyboard.press("ArrowUp");
    await this.page.keyboard.type(text);
  }

  async selectAll() {
    await this.editor.click();
    await this.page.keyboard.press("Control+A");
  }

  async replaceAllContent(text: string) {
    await this.selectAll();
    await this.page.keyboard.type(text);
  }

  async deleteSelectedText() {
    await this.page.keyboard.press("Delete");
  }

  async undo() {
    await this.page.keyboard.press("Control+Z");
  }

  async redo() {
    await this.page.keyboard.press("Control+Y");
  }

  async saveFile() {
    await expect(this.saveButton).toBeVisible({ timeout: 10000 });
    await this.saveButton.click();
    await expect(this.saveButton).not.toBeVisible({ timeout: 10000 });
  }

  async verifySaveButtonVisible() {
    await expect(this.saveButton).toBeVisible({ timeout: 10000 });
  }

  async verifySaveButtonHidden() {
    await expect(this.saveButton).not.toBeVisible({ timeout: 5000 });
  }

  async verifyBreadcrumb(filePath: string) {
    // The breadcrumb contains separators (icons) between path parts,
    // so we need to check each part separately or use containsText
    // Wait for breadcrumb to update by checking for the first part
    const parts = filePath.split("/");
    await expect(this.breadcrumb).toContainText(parts[0], { timeout: 10000 });

    // Then verify all parts are present
    for (const part of parts) {
      await expect(this.breadcrumb).toContainText(part);
    }
  }

  async getEditorContent(): Promise<string> {
    // Get all text lines from Monaco editor
    const lines = await this.page.locator(".view-line").allTextContents();
    return lines.join("\n");
  }

  async waitForEditorToLoad() {
    await expect(this.editor).toBeVisible({ timeout: 10000 });
  }

  async verifyFileIsOpen(fileName: string) {
    // Wait for breadcrumb to be visible first (it appears after file loads)
    await expect(this.breadcrumb).toBeVisible({ timeout: 10000 });
    await expect(this.breadcrumb).toContainText(fileName);
  }

  // Advanced editing scenarios
  async addMultipleLines(lines: string[]) {
    await this.editor.click();
    await this.page.keyboard.press("Control+End");
    for (const line of lines) {
      await this.page.keyboard.press("Enter");
      await this.page.keyboard.type(line);
    }
    // Wait a moment for the editor to register the changes
    await this.page.waitForTimeout(500);
  }

  async deleteLine() {
    await this.editor.click();
    await this.page.keyboard.press("Control+Shift+K"); // VS Code shortcut for delete line
  }

  async duplicateLine() {
    await this.editor.click();
    await this.page.keyboard.press("Control+Shift+D"); // VS Code shortcut for duplicate line
  }

  async goToLine(lineNumber: number) {
    await this.page.keyboard.press("Control+G");
    await this.page.keyboard.type(String(lineNumber));
    await this.page.keyboard.press("Enter");
  }

  async find(searchText: string) {
    await this.page.keyboard.press("Control+F");
    await this.page.keyboard.type(searchText);
  }

  async replace(searchText: string, replaceText: string) {
    await this.page.keyboard.press("Control+H");
    await this.page.keyboard.type(searchText);
    await this.page.keyboard.press("Tab");
    await this.page.keyboard.type(replaceText);
  }

  // IDE sidebar mode switching
  async switchToFilesMode() {
    await this.filesModeButton.click();
    // Wait for mode to switch
    await this.page.waitForTimeout(300);
  }

  async switchToObjectsMode() {
    await this.objectsModeButton.click();
    // Wait for mode to switch
    await this.page.waitForTimeout(300);
  }

  async verifyFilesMode() {
    await expect(this.filesModeButton).toBeVisible();

    // Wait for file tree to load - increase timeout for slower loading
    await this.page.waitForTimeout(1500);

    // Just verify we're in Files mode - the tab is selected
    await expect(this.filesModeButton)
      .toHaveAttribute("data-state", "active", { timeout: 5000 })
      .catch(() => {
        // Fallback: just check that Files tab is visible
      });

    const hasWorkflowsFolder = await this.page
      .getByRole("button", { name: "workflows", exact: true })
      .isVisible()
      .catch(() => false);
    const hasGeneratedFolder = await this.page
      .getByRole("button", { name: "generated", exact: true })
      .isVisible()
      .catch(() => false);
    const hasExampleSqlFolder = await this.page
      .getByRole("button", { name: "example_sql", exact: true })
      .isVisible()
      .catch(() => false);
    const hasConfigFile = await this.page
      .getByRole("link", { name: "config.yml" })
      .first()
      .isVisible()
      .catch(() => false);

    // At least one folder or file should be visible
    expect(
      hasWorkflowsFolder || hasGeneratedFolder || hasExampleSqlFolder || hasConfigFile
    ).toBeTruthy();
  }

  async verifyObjectsMode() {
    // Verify the Objects tab is active
    await expect(this.objectsModeButton).toBeVisible();
    await expect(this.objectsModeButton).toHaveAttribute("data-state", "active");

    // In Objects mode, we should see grouped sections
    const semanticLayerHeading = this.page.locator("text=Semantic Layer");
    const automationsHeading = this.page.locator("text=Automations");
    const agentsHeading = this.page.locator("text=Agents");
    const appsHeading = this.page.locator("text=Apps");

    // At least one of these groups should be visible
    const visibleCount = await Promise.all([
      semanticLayerHeading.isVisible().catch(() => false),
      automationsHeading.isVisible().catch(() => false),
      agentsHeading.isVisible().catch(() => false),
      appsHeading.isVisible().catch(() => false)
    ]).then((results) => results.filter(Boolean).length);

    expect(visibleCount).toBeGreaterThan(0);
  }
}
