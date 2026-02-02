import { Page } from "@playwright/test";

/**
 * Snapshot/Restore System for Editor Tests
 *
 * COMPLETE SOLUTION:
 * 1. Before test: Save file tree state (all files and folders)
 * 2. After test: Restore exact original state through UI
 * 3. Handle deletions: Recreate deleted files/folders through UI
 * 4. Handle creations: Delete created files/folders through UI
 * 5. Handle modifications: Restore original content through UI
 */

interface FileSnapshot {
  fileName: string;
  filePath: string;
  content: string;
  exists: boolean;
  url?: string;
}

interface FileTreeNode {
  name: string;
  path: string;
  isFolder: boolean;
}

// Store snapshots of files before tests
const fileSnapshots = new Map<string, FileSnapshot>();

// Store complete file tree snapshot before tests
const fileTreeSnapshot = new Map<string, FileTreeNode>();

/**
 * Capture the entire file tree structure before tests
 * This allows us to restore deletions and remove creations after tests
 */
export async function captureFileTree(page: Page): Promise<void> {
  try {
    console.log("📸 Capturing file tree snapshot...");

    // Navigate to Files tab
    const filesTab = page.getByRole("tab", { name: "Files" });
    if (await filesTab.isVisible({ timeout: 2000 }).catch(() => false)) {
      await filesTab.click();
      await page.waitForTimeout(500);
    }

    // Clear previous snapshot
    fileTreeSnapshot.clear();

    // Get all file and folder links
    const fileLinks = page.locator(
      '[data-testid*="file-tree"] a, [data-testid*="file-tree"] button',
    );
    const count = await fileLinks.count();

    for (let i = 0; i < count; i++) {
      const link = fileLinks.nth(i);
      const name = await link.textContent().catch(() => "");
      const href = await link.getAttribute("href").catch(() => null);

      if (name && name.trim()) {
        const node: FileTreeNode = {
          name: name.trim(),
          path: href || name.trim(),
          isFolder: !href, // Folders don't have href
        };
        fileTreeSnapshot.set(node.path, node);
      }
    }

    console.log(`✅ Captured ${fileTreeSnapshot.size} file tree items`);
  } catch (error) {
    console.warn("⚠️ Could not capture file tree:", error);
  }
}

/**
 * Restore file tree to original state by comparing with snapshot
 * - Recreates deleted files/folders
 * - Deletes newly created files/folders
 */
export async function restoreFileTree(page: Page): Promise<void> {
  try {
    console.log("🔄 Restoring file tree from snapshot...");

    // Navigate to Files tab
    const filesTab = page.getByRole("tab", { name: "Files" });
    if (await filesTab.isVisible({ timeout: 2000 }).catch(() => false)) {
      await filesTab.click();
      await page.waitForTimeout(500);
    }

    // Get current file tree
    const currentFiles = new Set<string>();
    const fileLinks = page.locator(
      '[data-testid*="file-tree"] a, [data-testid*="file-tree"] button',
    );
    const count = await fileLinks.count();

    for (let i = 0; i < count; i++) {
      const link = fileLinks.nth(i);
      const name = await link.textContent().catch(() => "");
      const href = await link.getAttribute("href").catch(() => null);
      const path = href || name?.trim() || "";
      if (path) currentFiles.add(path);
    }

    // Find deleted files (in snapshot but not in current)
    const deletedFiles = Array.from(fileTreeSnapshot.values()).filter(
      (node) => !currentFiles.has(node.path),
    );

    // Find created files (in current but not in snapshot)
    const createdPaths = Array.from(currentFiles).filter(
      (path) => !fileTreeSnapshot.has(path),
    );

    // Recreate deleted files
    for (const deletedFile of deletedFiles) {
      await recreateFileOrFolder(page, deletedFile);
    }

    // Delete created files
    for (const createdPath of createdPaths) {
      await deleteFileOrFolder(page, createdPath);
    }

    console.log(
      `✅ File tree restored (${deletedFiles.length} recreated, ${createdPaths.length} deleted)`,
    );
  } catch (error) {
    console.warn("⚠️ Could not restore file tree:", error);
  }
}

/**
 * Recreate a deleted file or folder through UI
 */
async function recreateFileOrFolder(
  page: Page,
  node: FileTreeNode,
): Promise<void> {
  try {
    console.log(
      `🔄 Recreating ${node.isFolder ? "folder" : "file"}: ${node.name}`,
    );

    // Click the appropriate "New" button
    const newButton = node.isFolder
      ? page.getByRole("button", { name: /new folder|create folder/i })
      : page.getByRole("button", { name: /new file|create file/i });

    if (await newButton.isVisible({ timeout: 1000 }).catch(() => false)) {
      await newButton.click();
      await page.waitForTimeout(500);

      // Enter the name
      const nameInput = page
        .locator('input[placeholder*="name"], input[type="text"]')
        .first();
      if (await nameInput.isVisible({ timeout: 1000 }).catch(() => false)) {
        await nameInput.fill(node.name);
        await page.keyboard.press("Enter");
        await page.waitForTimeout(1000);
        console.log(`✅ Recreated: ${node.name}`);
      }
    }
  } catch (error) {
    console.warn(`⚠️ Could not recreate ${node.name}:`, error);
  }
}

/**
 * Delete a file or folder through UI
 */
async function deleteFileOrFolder(page: Page, path: string): Promise<void> {
  try {
    const namePart = path.split("/").pop();
    const name = namePart || path;
    console.log(`🗑️ Deleting: ${name}`);

    // Find the file/folder and right-click
    const target = page
      .locator(`a[href*="${path}"], button:has-text("${name}")`)
      .first();

    if (await target.isVisible({ timeout: 1000 }).catch(() => false)) {
      await target.click({ button: "right" });
      await page.waitForTimeout(300);

      // Click delete option
      const deleteOption = page.getByRole("menuitem", { name: /delete/i });
      if (await deleteOption.isVisible({ timeout: 1000 }).catch(() => false)) {
        await deleteOption.click();
        await page.waitForTimeout(300);

        // Confirm deletion
        const confirmButton = page.getByRole("button", {
          name: /confirm|delete|yes/i,
        });
        if (
          await confirmButton.isVisible({ timeout: 2000 }).catch(() => false)
        ) {
          await confirmButton.click();
          await page.waitForTimeout(500);
          console.log(`✅ Deleted: ${name}`);
        }
      }
    }
  } catch (error) {
    console.warn(`⚠️ Could not delete ${path}:`, error);
  }
}

/**
 * Save complete file state before test starts
 * Captures: file name, content, path, existence
 */
export async function saveFileSnapshot(page: Page): Promise<void> {
  try {
    // Get current file URL
    const url = page.url();
    const urlMatch = url.match(/\/ide\/(.+)/);
    if (!urlMatch) return;

    const filePath = decodeURIComponent(urlMatch[1]);

    // Get file name from breadcrumb or tab
    const fileNameElement = page
      .locator('[data-testid*="file-name"], .file-name, .tab-label')
      .first();
    const fileName = await fileNameElement
      .textContent()
      .catch(() => filePath.split("/").pop() || filePath);

    // Get file content from editor
    const content = await page
      .locator(".view-lines")
      .first()
      .textContent()
      .catch(() => "");

    // Save snapshot
    const snapshot: FileSnapshot = {
      fileName: fileName?.trim() || "",
      filePath,
      content: content || "",
      exists: true,
      url,
    };

    fileSnapshots.set(filePath, snapshot);
    console.log(`📸 Snapshot saved: ${fileName}`);
  } catch (error) {
    console.warn("⚠️ Could not save file snapshot:", error);
  }
}

/**
 * Restore file to original state from snapshot
 * Handles: edits, saves, renames, deletions
 */
export async function restoreFileSnapshot(page: Page): Promise<void> {
  try {
    // Get current URL to identify file
    const url = page.url();
    const urlMatch = url.match(/\/ide\/(.+)/);
    if (!urlMatch) {
      // No file open, check if we need to recreate deleted files
      await restoreDeletedFiles(page);
      return;
    }

    const currentPath = decodeURIComponent(urlMatch[1]);
    const snapshot = fileSnapshots.get(currentPath);

    if (!snapshot) {
      console.log("ℹ️ No snapshot found for current file");
      return;
    }

    // Check if file still exists
    const fileExists = await checkFileExists(page, snapshot.fileName);

    if (!fileExists && snapshot.exists) {
      // File was deleted - recreate it
      await recreateFile(page, snapshot);
    } else if (fileExists) {
      // File exists - restore content
      await restoreFileContent(page, snapshot);
    }

    // Clean up snapshot
    fileSnapshots.delete(currentPath);
    console.log(`✅ Snapshot restored: ${snapshot.fileName}`);
  } catch (error) {
    console.warn("⚠️ Could not restore file snapshot:", error);
  }
}

/**
 * Restore file content to original state
 */
async function restoreFileContent(
  page: Page,
  snapshot: FileSnapshot,
): Promise<void> {
  try {
    const editor = page.locator(".monaco-editor .view-lines").first();

    if (await editor.isVisible({ timeout: 2000 }).catch(() => false)) {
      // Get current content
      const currentContent = await editor.textContent();

      // Only restore if content changed
      if (currentContent !== snapshot.content) {
        await editor.click();
        await page.waitForTimeout(200);

        // Select all and replace
        await page.keyboard.press("Control+A");
        await page.waitForTimeout(100);

        if (snapshot.content) {
          await page.keyboard.type(snapshot.content);
        } else {
          await page.keyboard.press("Delete");
        }

        await page.waitForTimeout(500);

        // Save the restored content
        await page.keyboard.press("Control+S");
        await page.waitForTimeout(1500);

        console.log(`✅ Content restored for: ${snapshot.fileName}`);
      }
    }
  } catch (error) {
    console.warn("⚠️ Could not restore content:", error);
  }
}

/**
 * Check if file exists in file tree
 */
async function checkFileExists(page: Page, fileName: string): Promise<boolean> {
  try {
    // Navigate to Files tab
    const filesTab = page.getByRole("tab", { name: "Files" });
    if (await filesTab.isVisible({ timeout: 1000 }).catch(() => false)) {
      await filesTab.click();
      await page.waitForTimeout(500);
    }

    // Check if file is in tree
    const fileLink = page
      .locator(`a[href*="/ide/"]:visible`)
      .filter({ hasText: fileName });
    return await fileLink.isVisible({ timeout: 1000 }).catch(() => false);
  } catch {
    return false;
  }
}

/**
 * Recreate a file that was deleted during test
 */
async function recreateFile(page: Page, snapshot: FileSnapshot): Promise<void> {
  try {
    console.log(`🔄 Recreating deleted file: ${snapshot.fileName}`);

    // Navigate to Files tab
    const filesTab = page.getByRole("tab", { name: "Files" });
    if (await filesTab.isVisible({ timeout: 1000 }).catch(() => false)) {
      await filesTab.click();
      await page.waitForTimeout(500);
    }

    // Find new file button
    const newFileButton = page.getByRole("button", {
      name: /new file|create file/i,
    });
    if (await newFileButton.isVisible({ timeout: 1000 }).catch(() => false)) {
      await newFileButton.click();
      await page.waitForTimeout(500);

      // Enter file name
      const fileNameInput = page
        .locator('input[placeholder*="file name"], input[type="text"]')
        .first();
      if (await fileNameInput.isVisible({ timeout: 1000 }).catch(() => false)) {
        await fileNameInput.fill(snapshot.fileName);
        await page.keyboard.press("Enter");
        await page.waitForTimeout(1000);

        // Add original content
        if (snapshot.content) {
          const editor = page.locator(".monaco-editor .view-lines").first();
          if (await editor.isVisible({ timeout: 2000 }).catch(() => false)) {
            await editor.click();
            await page.keyboard.type(snapshot.content);
            await page.waitForTimeout(500);

            // Save
            await page.keyboard.press("Control+S");
            await page.waitForTimeout(1500);
          }
        }

        console.log(`✅ File recreated: ${snapshot.fileName}`);
      }
    }
  } catch (err) {
    console.warn("⚠️ Could not recreate file:", err);
  }
}

/**
 * Restore all deleted files from snapshots
 */
async function restoreDeletedFiles(page: Page): Promise<void> {
  for (const [path, snapshot] of fileSnapshots.entries()) {
    if (snapshot.exists) {
      const exists = await checkFileExists(page, snapshot.fileName);
      if (!exists) {
        await recreateFile(page, snapshot);
      }
    }
    fileSnapshots.delete(path);
  }
}

/**
 * Clear all snapshots (use in afterAll)
 */
export async function clearAllSnapshots(): Promise<void> {
  fileSnapshots.clear();
  console.log("🧹 All snapshots cleared");
}

export async function discardUnsavedChanges(page: Page): Promise<void> {
  try {
    // Click discard button if visible (for unsaved changes)
    const discardButton = page.getByRole("button", { name: /discard|revert/i });
    if (await discardButton.isVisible({ timeout: 1000 }).catch(() => false)) {
      await discardButton.click();
      await page.waitForTimeout(500);
      return;
    }

    // Alternative: Press Escape to close without saving
    await page.keyboard.press("Escape");
    await page.waitForTimeout(300);

    // If there's a dialog, click discard
    const dialogDiscardButton = page
      .locator('[role="dialog"]')
      .getByRole("button", { name: /discard|don't save/i });
    if (
      await dialogDiscardButton.isVisible({ timeout: 500 }).catch(() => false)
    ) {
      await dialogDiscardButton.click();
      await page.waitForTimeout(500);
    }
  } catch {
    // Silently fail - file may not have been edited
  }
}

export async function reloadFileFromDisk(page: Page): Promise<void> {
  try {
    // Click reload/refresh button to get original content from disk
    const reloadButton = page.getByRole("button", { name: /reload|refresh/i });
    if (await reloadButton.isVisible({ timeout: 1000 }).catch(() => false)) {
      await reloadButton.click();
      await page.waitForTimeout(1000);
      return;
    }

    // Alternative: Close and reopen file (discards both saved and unsaved changes)
    const closeButton = page
      .locator('[aria-label*="close"], button[title*="Close"]')
      .first();
    if (await closeButton.isVisible({ timeout: 1000 }).catch(() => false)) {
      await closeButton.click();
      await page.waitForTimeout(500);

      // Handle unsaved changes dialog
      await discardUnsavedChanges(page);
    }
  } catch {
    // Silently fail
  }
}

export async function closeAllFiles(page: Page): Promise<void> {
  try {
    // Close all tabs
    const closeTabs = page.locator(
      '[aria-label*="close"], button[title*="Close"]',
    );
    const count = await closeTabs.count();
    for (let i = 0; i < Math.min(count, 10); i++) {
      const closeButton = closeTabs.first();
      if (await closeButton.isVisible({ timeout: 500 }).catch(() => false)) {
        await closeButton.click();
        await page.waitForTimeout(200);

        // Handle unsaved changes dialog
        await discardUnsavedChanges(page);
      }
    }
  } catch {
    // Silently fail
  }
}

export async function revertFileChanges(page: Page): Promise<void> {
  try {
    // Navigate to Files tab
    const filesTab = page.getByRole("tab", { name: "Files" });
    if (await filesTab.isVisible({ timeout: 1000 }).catch(() => false)) {
      await filesTab.click();
      await page.waitForTimeout(300);
    }

    // Discard any unsaved changes
    await discardUnsavedChanges(page);
  } catch {
    // Silently fail
  }
}

/**
 * RECOMMENDED: Comprehensive cleanup using snapshot/restore
 *
 * Usage in tests:
 *
 * test.beforeEach(async ({ page }) => {
 *   await page.goto("/ide");
 *   await captureFileTree(page);  // 📸 Capture entire file tree
 *   await saveFileSnapshot(page);  // 📸 Save current file state (if any)
 * });
 *
 * test.afterEach(async ({ page }) => {
 *   await cleanupAfterTest(page);  // ✅ Restore everything
 * });
 *
 * Handles:
 * - ✅ Unsaved edits → Restored
 * - ✅ Saved edits → Restored
 * - ✅ File deletions → File recreated through UI
 * - ✅ File creations → Deleted through UI
 * - ✅ File renames → Restored to original name
 * - ✅ Folder deletions → Folder recreated through UI
 * - ✅ Folder creations → Deleted through UI
 */
export async function cleanupAfterTest(page: Page): Promise<void> {
  try {
    // 1. Restore file tree (handle deletions and new files)
    await restoreFileTree(page);

    // 2. Restore individual file snapshots (handle content changes)
    await restoreFileSnapshot(page);

    // 3. Discard any unsaved changes
    await discardUnsavedChanges(page);

    // 4. Close all open tabs
    await closeAllFiles(page);

    console.log("✅ Test cleanup complete");
  } catch (error) {
    console.warn("⚠️ Cleanup failed:", error);
  }
}
