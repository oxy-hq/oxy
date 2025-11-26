import { test, expect } from "@playwright/test";
import { resetProject } from "./utils";

test.describe("Workflow Execution", () => {
  test.beforeEach(async ({ page }) => {
    resetProject();
    await page.goto("/");
  });

  test("should be able to run a workflow and see the result", async ({
    page,
  }) => {
    // Navigate to the table_values workflow
    await page.getByTestId("workflow-link-table_values").click();

    // Click the Play button to run the workflow
    await page.getByTestId("run-workflow-button").click();

    // Wait for the workflow to complete by monitoring the API event stream
    // The last event should be {"type":"workflow_finished",...}
    await page.waitForResponse(
      (response) => {
        const url = response.url();
        return (
          url.includes("/api/") &&
          url.includes("/events") &&
          url.includes("source_id=workflows%2Ftable_values.workflow.yml") &&
          response.status() === 200
        );
      },
      { timeout: 60000 },
    );
    // Verify OutputLogs is visible
    await expect(page.getByTestId("workflow-output-logs")).toBeVisible({
      timeout: 10000,
    });

    // Verify OutputItem (with Markdown content) is visible
    await expect(page.getByTestId("workflow-output-item").first()).toBeVisible({
      timeout: 10000,
    });

    // Verify the StatusBorder of DiagramNode has border-emerald-600 (success state)
    const statusBorder = page
      .getByTestId("workflow-node-status-border")
      .first();
    await expect(statusBorder).toBeVisible({ timeout: 10000 });
    await expect(statusBorder).toHaveClass(/border-emerald-600/);
  });
});
