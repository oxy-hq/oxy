import { Page, Locator, expect } from "@playwright/test";

export class IDEPage {
  readonly page: Page;
  readonly editor: Locator;
  readonly saveButton: Locator;
  readonly breadcrumb: Locator;

  constructor(page: Page) {
    this.page = page;
    this.editor = page.locator(".monaco-editor");
    this.saveButton = page.getByTestId("ide-save-button");
    this.breadcrumb = page.getByTestId("ide-breadcrumb");
  }

  async goto() {
    await this.page.goto("/ide");
  }

  async openFile(fileName: string) {
    await this.page.getByRole("link", { name: fileName }).click();
    await this.page.waitForURL(/\/ide\/.+/);
    await expect(this.editor).toBeVisible({ timeout: 10000 });
  }

  async expandFolder(folderName: string, knownFile?: string) {
    const folder = this.page.getByRole("button", {
      name: folderName,
      exact: true,
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
      exact: true,
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
    for (const part of filePath.split("/")) {
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
}
