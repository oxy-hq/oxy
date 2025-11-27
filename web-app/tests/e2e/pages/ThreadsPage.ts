import { Page, Locator, expect } from "@playwright/test";

export class ThreadsPage {
  readonly page: Page;
  readonly threadItems: Locator;
  readonly pagination: Locator;
  readonly itemsPerPageSelector: Locator;
  readonly selectModeButton: Locator;
  readonly selectedCountBadge: Locator;

  constructor(page: Page) {
    this.page = page;
    this.threadItems = page.getByTestId("thread-item");
    this.pagination = page.getByTestId("threads-pagination");
    this.itemsPerPageSelector = page.getByTestId("threads-per-page-selector");
    this.selectModeButton = page.getByRole("button", { name: "Select" });
    this.selectedCountBadge = page.getByTestId("selected-count-badge");
  }

  async goto() {
    await this.page.goto("/threads");
  }

  async verifyThreadsVisible(count?: number) {
    if (count !== undefined) {
      await expect(this.threadItems).toHaveCount(count);
    } else {
      await expect(this.threadItems.first()).toBeVisible();
    }
  }

  async clickThread(index: number = 0) {
    await this.threadItems.nth(index).click();
    await this.page.waitForURL(/\/threads\/.+/);
  }

  async findThreadByTitle(title: string) {
    return this.page.getByTestId("thread-title").filter({ hasText: title });
  }

  async clickThreadByTitle(title: string) {
    const thread = await this.findThreadByTitle(title);
    await thread.click();
  }

  async verifyThreadHasAgent(index: number, agentType: string) {
    const agentBadge = this.threadItems
      .nth(index)
      .getByTestId("thread-agent-type");
    await expect(agentBadge).toContainText(agentType);
  }

  async verifyPaginationVisible() {
    await expect(this.pagination).toBeVisible();
  }

  async goToPage(pageNumber: number) {
    await this.page.getByRole("link", { name: String(pageNumber) }).click();
    await this.page.waitForURL(new RegExp(`page=${pageNumber}`));
  }

  async clickNextPage() {
    await this.page.getByRole("button", { name: /next/i }).click();
  }

  async clickPreviousPage() {
    await this.page.getByRole("button", { name: /previous/i }).click();
  }

  async verifyCurrentPage(pageNumber: number) {
    const currentPageLink = this.page.getByRole("link", {
      name: String(pageNumber),
    });
    await expect(currentPageLink).toHaveAttribute("aria-current", "page");
  }

  async verifyNextButtonDisabled() {
    await expect(
      this.page.getByRole("button", { name: /next/i }),
    ).toBeDisabled();
  }

  async verifyPreviousButtonDisabled() {
    await expect(
      this.page.getByRole("button", { name: /previous/i }),
    ).toBeDisabled();
  }

  async selectItemsPerPage(count: number) {
    await this.itemsPerPageSelector.click();
    await this.page
      .getByRole("menuitemcheckbox", { name: String(count) })
      .click();
    await this.page.waitForURL(new RegExp(`limit=${count}`));
  }

  async enterSelectMode() {
    await this.selectModeButton.click();
  }

  async selectThread(index: number) {
    const checkbox = this.threadItems
      .nth(index)
      .locator('input[type="checkbox"]');
    await checkbox.click();
  }

  async verifySelectedCount(count: number) {
    await expect(this.selectedCountBadge).toContainText(String(count));
  }

  async deleteSelectedThreads() {
    await this.page.getByRole("button", { name: /delete/i }).click();
  }

  async getThreadCount(): Promise<number> {
    return await this.threadItems.count();
  }

  async verifyThreadTimestamp(index: number) {
    const timestamp = this.threadItems
      .nth(index)
      .getByTestId("thread-timestamp");
    await expect(timestamp).toBeVisible();
  }
}
