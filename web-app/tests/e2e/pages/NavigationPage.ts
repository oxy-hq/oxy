import { expect, type Locator, type Page } from "@playwright/test";

export class NavigationPage {
  readonly page: Page;
  readonly homeLink: Locator;
  readonly threadsLink: Locator;
  readonly ontologyLink: Locator;
  readonly ideLink: Locator;
  readonly workflowsLink: Locator;
  readonly appsLink: Locator;

  constructor(page: Page) {
    this.page = page;
    this.homeLink = page.getByRole("link", { name: "Home" });
    this.threadsLink = page.getByRole("link", { name: "Threads" });
    this.ontologyLink = page.getByRole("link", { name: "Ontology" });
    this.ideLink = page.getByRole("link", { name: "IDE" });
    this.workflowsLink = page.getByRole("link", { name: "Automations" });
    this.appsLink = page.getByRole("link", { name: "Apps" });
  }

  async goToHome() {
    await this.homeLink.click();
    await expect(this.page).toHaveURL("/");
  }

  async goToThreads() {
    await this.threadsLink.click();
    await expect(this.page).toHaveURL(/\/threads/);
  }

  async goToOntology() {
    await this.ontologyLink.click();
    await expect(this.page).toHaveURL(/\/ontology/);
  }

  async goToIDE() {
    await this.ideLink.click();
    await expect(this.page).toHaveURL(/\/ide/);
  }

  async verifyPageLoaded(pageName: string) {
    await expect(this.page.getByRole("heading", { name: pageName, level: 1 })).toBeVisible();
  }

  async verifySidebarThreadLinkExists(threadId: string) {
    await expect(this.page.getByTestId(`sidebar-thread-link-${threadId}`)).toBeVisible();
  }

  async clickSidebarThreadLink(threadId: string) {
    await this.page.getByTestId(`sidebar-thread-link-${threadId}`).click();
    await this.page.waitForURL(/\/threads\/.+/);
  }

  async deleteSidebarThread(threadId: string) {
    await this.page.getByTestId(`sidebar-thread-delete-${threadId}`).click();
  }

  async toggleSidebarThreads() {
    await this.page.getByTestId("sidebar-threads-toggle").click();
  }

  async verifyWorkflowLink(workflowName: string) {
    await expect(this.page.getByTestId(`workflow-link-${workflowName}`)).toBeVisible();
  }

  async clickWorkflowLink(workflowName: string) {
    await this.page.getByTestId(`workflow-link-${workflowName}`).click();
  }

  async verifyAppLink(appName: string) {
    await expect(this.page.getByTestId(`app-link-${appName}`)).toBeVisible();
  }

  async clickAppLink(appName: string) {
    await this.page.getByTestId(`app-link-${appName}`).click();
  }
}
