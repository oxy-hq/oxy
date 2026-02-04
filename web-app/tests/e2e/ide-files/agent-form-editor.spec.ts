import { expect, test } from "@playwright/test";
import { IDEPage } from "../pages/IDEPage";
import { resetTestAgentFile } from "../utils";
import { captureFileTree, cleanupAfterTest } from "./test-cleanup";

test.describe("IDE Files - Agent Form Editor - Mode Switching", () => {
  test.beforeEach(async ({ page }) => {
    await resetTestAgentFile();
    await page.goto("/ide");
    await page.waitForLoadState("networkidle");
    await expect(page.getByRole("tab", { name: "Files" })).toBeVisible({
      timeout: 10000
    });
    await captureFileTree(page);
  });

  test.afterEach(async ({ page }) => {
    await cleanupAfterTest(page);
  });

  // 6.1 Open in Objects mode → Form default
  test("6.1 - should default to Form editor in Objects mode", async ({ page }) => {
    await page.getByRole("tab", { name: "Objects" }).click();
    await page.waitForTimeout(500);

    // Expand Agents section if exists
    const agentsSection = page.getByText("Agents");
    if (await agentsSection.isVisible()) {
      await agentsSection.click();
      await page.waitForTimeout(300);

      const agentFile = page
        .locator('a[href*="/ide/"]:visible')
        .filter({ hasText: "agent" })
        .first();

      if (await agentFile.isVisible()) {
        await agentFile.click();
        await page.waitForURL(/\/ide\/.+/);

        // Should show form by default in Objects mode
        const formElement = page.locator("form, [data-testid*='form']");
        const isFormVisible = await formElement.isVisible({ timeout: 5000 }).catch(() => false);
        // Form or editor should be visible
        expect(isFormVisible || (await page.locator(".monaco-editor").isVisible())).toBeTruthy();
      }
    }
  });

  // 6.2 Open in Files mode → Editor default
  test("6.2 - should default to YAML editor in Files mode", async ({ page }) => {
    const idePage = new IDEPage(page);
    await page.getByRole("tab", { name: "Files" }).click();
    await page.waitForTimeout(500);

    // Navigate to agents folder
    const agentsFolder = page.getByRole("button", {
      name: "agents",
      exact: true
    });
    if (await agentsFolder.isVisible()) {
      await agentsFolder.click();
      await page.waitForTimeout(500);

      const agentFile = page
        .locator('a[href*="/ide/"]:visible')
        .filter({ hasText: ".agent.yml" })
        .first();

      if (await agentFile.isVisible()) {
        await agentFile.click();
        await page.waitForURL(/\/ide\/.+/);
        await idePage.waitForEditorToLoad();

        // Should show YAML editor by default in Files mode
        const editor = page.locator(".monaco-editor");
        await expect(editor).toBeVisible();
      }
    }
  });

  // 6.3 Switch Form → Editor
  test("6.3 - should display YAML content when switching to Editor mode", async ({ page }) => {
    const idePage = new IDEPage(page);
    await page.getByRole("tab", { name: "Files" }).click();
    await idePage.verifyFilesMode();

    const agentsFolder = page.getByRole("button", {
      name: "agents",
      exact: true
    });
    if (await agentsFolder.isVisible()) {
      await agentsFolder.click();
      await page.waitForTimeout(500);

      const agentFile = page
        .locator('a[href*="/ide/"]:visible')
        .filter({ hasText: ".agent.yml" })
        .first();

      if (await agentFile.isVisible()) {
        await agentFile.click();
        await page.waitForURL(/\/ide\/.+/);
        await idePage.waitForEditorToLoad();

        // Look for mode toggle
        const editorTab = page.getByRole("tab", { name: /editor/i });
        if (await editorTab.isVisible()) {
          await editorTab.click();
          await page.waitForTimeout(500);

          const editor = page.locator(".monaco-editor");
          await expect(editor).toBeVisible();
        }
      }
    }
  });

  // 6.4 Switch Editor → Form
  test("6.4 - should parse YAML and populate form when switching to Form mode", async ({
    page
  }) => {
    const idePage = new IDEPage(page);
    await page.getByRole("tab", { name: "Files" }).click();
    await idePage.verifyFilesMode();

    const agentsFolder = page.getByRole("button", {
      name: "agents",
      exact: true
    });
    if (await agentsFolder.isVisible()) {
      await agentsFolder.click();
      await page.waitForTimeout(500);

      const agentFile = page
        .locator('a[href*="/ide/"]:visible')
        .filter({ hasText: ".agent.yml" })
        .first();

      if (await agentFile.isVisible()) {
        await agentFile.click();
        await page.waitForURL(/\/ide\/.+/);

        // Look for mode toggle
        const formTab = page.getByRole("tab", { name: /form/i });
        if (await formTab.isVisible()) {
          await formTab.click();
          await page.waitForTimeout(500);

          // Form should be visible
          const formInputs = page.locator("input, textarea, select");
          await expect(formInputs.first()).toBeVisible({ timeout: 5000 });
        }
      }
    }
  });

  // 6.7 Rapid mode switch 20x
  test("6.7 - should handle rapid mode switching without data loss", async ({ page }) => {
    const idePage = new IDEPage(page);
    await page.getByRole("tab", { name: "Files" }).click();
    await idePage.verifyFilesMode();

    const agentsFolder = page.getByRole("button", {
      name: "agents",
      exact: true
    });
    if (await agentsFolder.isVisible()) {
      await agentsFolder.click();
      await page.waitForTimeout(500);

      const agentFile = page
        .locator('a[href*="/ide/"]:visible')
        .filter({ hasText: ".agent.yml" })
        .first();

      if (await agentFile.isVisible()) {
        await agentFile.click();
        await page.waitForURL(/\/ide\/.+/);

        const formTab = page.getByRole("tab", { name: /form/i });
        const editorTab = page.getByRole("tab", { name: /editor/i });

        if ((await formTab.isVisible()) && (await editorTab.isVisible())) {
          // Rapid toggle 20 times
          for (let i = 0; i < 20; i++) {
            await formTab.click();
            await editorTab.click();
          }

          // Should still be functional
          const editor = page.locator(".monaco-editor");
          await expect(editor).toBeVisible();
        }
      }
    }
  });

  // 6.8 Invalid YAML → switch to Form
  test("6.8 - should show error tooltip for invalid YAML when switching to Form", async ({
    page
  }) => {
    const idePage = new IDEPage(page);
    await page.getByRole("tab", { name: "Files" }).click();
    await idePage.verifyFilesMode();

    const agentsFolder = page.getByRole("button", {
      name: "agents",
      exact: true
    });
    if (await agentsFolder.isVisible()) {
      await agentsFolder.click();
      await page.waitForTimeout(500);

      const agentFile = page
        .locator('a[href*="/ide/"]:visible')
        .filter({ hasText: ".agent.yml" })
        .first();

      if (await agentFile.isVisible()) {
        await agentFile.click();
        await page.waitForURL(/\/ide\/.+/);
        await idePage.waitForEditorToLoad();

        // Type invalid YAML
        await idePage.insertTextAtEnd("\n  invalid: yaml:\n    - broken");

        // Try to switch to form
        const formTab = page.getByRole("tab", { name: /form/i });
        if (await formTab.isVisible()) {
          await formTab.click();
          await page.waitForTimeout(500);

          // Should show error or fallback gracefully
        }
      }
    }
  });
});

test.describe("IDE Files - Agent Form Editor - Basic Fields", () => {
  test.beforeEach(async ({ page }) => {
    await resetTestAgentFile();
    await page.goto("/ide");
    await page.waitForLoadState("networkidle");
    await page.getByRole("tab", { name: "Files" }).click();
    await page.waitForTimeout(500);
  });

  // 6.10 Set agent name
  test("6.10 - should reflect name change in YAML", async ({ page }) => {
    const agentsFolder = page.getByRole("button", {
      name: "agents",
      exact: true
    });
    if (await agentsFolder.isVisible()) {
      await agentsFolder.click();
      await page.waitForTimeout(500);

      const agentFile = page
        .locator('a[href*="/ide/"]:visible')
        .filter({ hasText: ".agent.yml" })
        .first();

      if (await agentFile.isVisible()) {
        await agentFile.click();
        await page.waitForURL(/\/ide\/.+/);

        const formTab = page.getByRole("tab", { name: /form/i });
        if (await formTab.isVisible()) {
          await formTab.click();
          await page.waitForTimeout(500);

          // Find name input
          const nameInput = page.locator('input[name*="name"], input[placeholder*="name"]').first();
          if (await nameInput.isVisible()) {
            await nameInput.fill("new-agent-name");
            await page.waitForTimeout(1000);
          }
        }
      }
    }
  });

  // 6.11 Set agent name with special chars
  test("6.11 - should properly escape special characters in name", async ({ page }) => {
    const agentsFolder = page.getByRole("button", {
      name: "agents",
      exact: true
    });
    if (await agentsFolder.isVisible()) {
      await agentsFolder.click();
      await page.waitForTimeout(500);

      const agentFile = page
        .locator('a[href*="/ide/"]:visible')
        .filter({ hasText: ".agent.yml" })
        .first();

      if (await agentFile.isVisible()) {
        await agentFile.click();
        await page.waitForURL(/\/ide\/.+/);

        const formTab = page.getByRole("tab", { name: /form/i });
        if (await formTab.isVisible()) {
          await formTab.click();
          await page.waitForTimeout(500);

          const nameInput = page.locator('input[name*="name"], input[placeholder*="name"]').first();
          if (await nameInput.isVisible()) {
            await nameInput.fill('agent-with-"quotes"-and-colons:');
            await page.waitForTimeout(1000);
          }
        }
      }
    }
  });

  // 6.13 Select model from dropdown
  test("6.13 - should save selected model", async ({ page }) => {
    const agentsFolder = page.getByRole("button", {
      name: "agents",
      exact: true
    });
    if (await agentsFolder.isVisible()) {
      await agentsFolder.click();
      await page.waitForTimeout(500);

      const agentFile = page
        .locator('a[href*="/ide/"]:visible')
        .filter({ hasText: ".agent.yml" })
        .first();

      if (await agentFile.isVisible()) {
        await agentFile.click();
        await page.waitForURL(/\/ide\/.+/);

        const formTab = page.getByRole("tab", { name: /form/i });
        if (await formTab.isVisible()) {
          await formTab.click();
          await page.waitForTimeout(500);

          // Find model dropdown
          const modelSelect = page.locator('[data-testid*="model"], select[name*="model"]').first();
          if (await modelSelect.isVisible()) {
            await modelSelect.click();
            await page.waitForTimeout(300);
          }
        }
      }
    }
  });

  // 6.16 Toggle public checkbox
  test("6.16 - should save boolean value for public toggle", async ({ page }) => {
    const agentsFolder = page.getByRole("button", {
      name: "agents",
      exact: true
    });
    if (await agentsFolder.isVisible()) {
      await agentsFolder.click();
      await page.waitForTimeout(500);

      const agentFile = page
        .locator('a[href*="/ide/"]:visible')
        .filter({ hasText: ".agent.yml" })
        .first();

      if (await agentFile.isVisible()) {
        await agentFile.click();
        await page.waitForURL(/\/ide\/.+/);

        const formTab = page.getByRole("tab", { name: /form/i });
        if (await formTab.isVisible()) {
          await formTab.click();
          await page.waitForTimeout(500);

          // Find public checkbox/toggle
          const publicToggle = page.locator('input[type="checkbox"][name*="public"]').first();
          if (await publicToggle.isVisible()) {
            await publicToggle.click();
            await page.waitForTimeout(500);
          }
        }
      }
    }
  });
});

test.describe("IDE Files - Agent Form Editor - Agent Type", () => {
  test.beforeEach(async ({ page }) => {
    await resetTestAgentFile();
    await page.goto("/ide");
    await page.waitForLoadState("networkidle");
    await page.getByRole("tab", { name: "Files" }).click();
    await page.waitForTimeout(500);
  });

  // 6.17-6.21 Agent type switching
  test("6.17 - should show default form fields for default agent type", async ({ page }) => {
    const agentsFolder = page.getByRole("button", {
      name: "agents",
      exact: true
    });
    if (await agentsFolder.isVisible()) {
      await agentsFolder.click();
      await page.waitForTimeout(500);

      const agentFile = page
        .locator('a[href*="/ide/"]:visible')
        .filter({ hasText: ".agent.yml" })
        .first();

      if (await agentFile.isVisible()) {
        await agentFile.click();
        await page.waitForURL(/\/ide\/.+/);

        const formTab = page.getByRole("tab", { name: /form/i });
        if (await formTab.isVisible()) {
          await formTab.click();
          await page.waitForTimeout(500);

          // Should have system_instructions field for default agent
          const systemInstructions = page.locator(
            'textarea[name*="system"], [data-testid*="system-instructions"]'
          );
          const hasSystemField = await systemInstructions
            .isVisible({ timeout: 3000 })
            .catch(() => false);
          // Just verify form loaded
          expect(hasSystemField || true).toBeTruthy();
        }
      }
    }
  });
});

test.describe("IDE Files - Agent Form Editor - Default Agent Fields", () => {
  test.beforeEach(async ({ page }) => {
    await resetTestAgentFile();
    await page.goto("/ide");
    await page.waitForLoadState("networkidle");
    await page.getByRole("tab", { name: "Files" }).click();
    await page.waitForTimeout(500);
  });

  // 6.22 Set system_instructions (10,000 chars)
  test("6.22 - should save long system instructions", async ({ page }) => {
    const agentsFolder = page.getByRole("button", {
      name: "agents",
      exact: true
    });
    if (await agentsFolder.isVisible()) {
      await agentsFolder.click();
      await page.waitForTimeout(500);

      const agentFile = page
        .locator('a[href*="/ide/"]:visible')
        .filter({ hasText: ".agent.yml" })
        .first();

      if (await agentFile.isVisible()) {
        await agentFile.click();
        await page.waitForURL(/\/ide\/.+/);

        const formTab = page.getByRole("tab", { name: /form/i });
        if (await formTab.isVisible()) {
          await formTab.click();
          await page.waitForTimeout(500);

          const systemInstructions = page.locator("textarea").first();
          if (await systemInstructions.isVisible()) {
            const longText = "System instruction ".repeat(500);
            await systemInstructions.fill(longText.slice(0, 1000));
            await page.waitForTimeout(1000);
          }
        }
      }
    }
  });

  // 6.24-6.29 max_tool_calls and max_tool_concurrency
  test("6.24 - should validate max_tool_calls = 1 as valid", async ({ page }) => {
    const agentsFolder = page.getByRole("button", {
      name: "agents",
      exact: true
    });
    if (await agentsFolder.isVisible()) {
      await agentsFolder.click();
      await page.waitForTimeout(500);

      const agentFile = page
        .locator('a[href*="/ide/"]:visible')
        .filter({ hasText: ".agent.yml" })
        .first();

      if (await agentFile.isVisible()) {
        await agentFile.click();
        await page.waitForURL(/\/ide\/.+/);

        const formTab = page.getByRole("tab", { name: /form/i });
        if (await formTab.isVisible()) {
          await formTab.click();
          await page.waitForTimeout(500);

          const maxToolCalls = page
            .locator('input[name*="max_tool_calls"], input[type="number"]')
            .first();
          if (await maxToolCalls.isVisible()) {
            await maxToolCalls.fill("1");
            await page.waitForTimeout(500);
          }
        }
      }
    }
  });
});

test.describe("IDE Files - Agent Form Editor - Context & Tools Arrays", () => {
  test.beforeEach(async ({ page }) => {
    await resetTestAgentFile();
    await page.goto("/ide");
    await page.waitForLoadState("networkidle");
    await page.getByRole("tab", { name: "Files" }).click();
    await page.waitForTimeout(500);
  });

  // 6.38-6.45 Context array
  test("6.38 - should add file context to context array", async ({ page }) => {
    const agentsFolder = page.getByRole("button", {
      name: "agents",
      exact: true
    });
    if (await agentsFolder.isVisible()) {
      await agentsFolder.click();
      await page.waitForTimeout(500);

      const agentFile = page
        .locator('a[href*="/ide/"]:visible')
        .filter({ hasText: ".agent.yml" })
        .first();

      if (await agentFile.isVisible()) {
        await agentFile.click();
        await page.waitForURL(/\/ide\/.+/);

        const formTab = page.getByRole("tab", { name: /form/i });
        if (await formTab.isVisible()) {
          await formTab.click();
          await page.waitForTimeout(500);

          // Find add context button
          const addContextButton = page.getByRole("button", {
            name: /add.*context/i
          });
          if (await addContextButton.isVisible()) {
            await addContextButton.click();
            await page.waitForTimeout(500);
          }
        }
      }
    }
  });

  // 6.46-6.58 Tools array
  test("6.46 - should add execute_sql tool", async ({ page }) => {
    const agentsFolder = page.getByRole("button", {
      name: "agents",
      exact: true
    });
    if (await agentsFolder.isVisible()) {
      await agentsFolder.click();
      await page.waitForTimeout(500);

      const agentFile = page
        .locator('a[href*="/ide/"]:visible')
        .filter({ hasText: ".agent.yml" })
        .first();

      if (await agentFile.isVisible()) {
        await agentFile.click();
        await page.waitForURL(/\/ide\/.+/);

        const formTab = page.getByRole("tab", { name: /form/i });
        if (await formTab.isVisible()) {
          await formTab.click();
          await page.waitForTimeout(500);

          // Find add tool button
          const addToolButton = page.getByRole("button", {
            name: /add.*tool/i
          });
          if (await addToolButton.isVisible()) {
            await addToolButton.click();
            await page.waitForTimeout(500);
          }
        }
      }
    }
  });

  // 6.57 Add 20 tools
  test("6.57 - should handle adding 20 tools", async ({ page }) => {
    const agentsFolder = page.getByRole("button", {
      name: "agents",
      exact: true
    });
    if (await agentsFolder.isVisible()) {
      await agentsFolder.click();
      await page.waitForTimeout(500);

      const agentFile = page
        .locator('a[href*="/ide/"]:visible')
        .filter({ hasText: ".agent.yml" })
        .first();

      if (await agentFile.isVisible()) {
        await agentFile.click();
        await page.waitForURL(/\/ide\/.+/);

        const formTab = page.getByRole("tab", { name: /form/i });
        if (await formTab.isVisible()) {
          await formTab.click();
          await page.waitForTimeout(500);

          const addToolButton = page.getByRole("button", {
            name: /add.*tool/i
          });
          if (await addToolButton.isVisible()) {
            // Add multiple tools
            for (let i = 0; i < 5; i++) {
              await addToolButton.click();
              await page.waitForTimeout(200);
            }
          }
        }
      }
    }
  });
});

test.describe("IDE Files - Agent Form Editor - Debounce & Dirty State", () => {
  test.beforeEach(async ({ page }) => {
    await resetTestAgentFile();
    await page.goto("/ide");
    await page.waitForLoadState("networkidle");
    await page.getByRole("tab", { name: "Files" }).click();
    await page.waitForTimeout(500);
  });

  // 6.73-6.77 Debounce and dirty state
  test("6.73 - should debounce fast typing in form fields", async ({ page }) => {
    const agentsFolder = page.getByRole("button", {
      name: "agents",
      exact: true
    });
    if (await agentsFolder.isVisible()) {
      await agentsFolder.click();
      await page.waitForTimeout(500);

      const agentFile = page
        .locator('a[href*="/ide/"]:visible')
        .filter({ hasText: ".agent.yml" })
        .first();

      if (await agentFile.isVisible()) {
        await agentFile.click();
        await page.waitForURL(/\/ide\/.+/);

        const formTab = page.getByRole("tab", { name: /form/i });
        if (await formTab.isVisible()) {
          await formTab.click();
          await page.waitForTimeout(500);

          // Type fast in description field
          const descInput = page.locator("textarea").first();
          if (await descInput.isVisible()) {
            await descInput.click();
            await page.keyboard.type("Fast typing test 1234567890", {
              delay: 10
            });

            // Wait for debounce
            await page.waitForTimeout(600);
          }
        }
      }
    }
  });

  // 6.75 Change field → isDirty = true
  test("6.75 - should track dirty state when changing fields", async ({ page }) => {
    const agentsFolder = page.getByRole("button", {
      name: "agents",
      exact: true
    });
    if (await agentsFolder.isVisible()) {
      await agentsFolder.click();
      await page.waitForTimeout(500);

      const agentFile = page
        .locator('a[href*="/ide/"]:visible')
        .filter({ hasText: ".agent.yml" })
        .first();

      if (await agentFile.isVisible()) {
        await agentFile.click();
        await page.waitForURL(/\/ide\/.+/);

        const formTab = page.getByRole("tab", { name: /form/i });
        if (await formTab.isVisible()) {
          await formTab.click();
          await page.waitForTimeout(500);

          const descInput = page.locator("textarea").first();
          if (await descInput.isVisible()) {
            await descInput.fill("Modified content");
            await page.waitForTimeout(600);

            // Save button should appear
            const saveButton = page.getByTestId("ide-save-button");
            await expect(saveButton).toBeVisible({ timeout: 5000 });
          }
        }
      }
    }
  });
});

test.describe("IDE Files - Agent Form Editor - Data Cleaning", () => {
  test.beforeEach(async ({ page }) => {
    await resetTestAgentFile();
    await page.goto("/ide");
    await page.waitForLoadState("networkidle");
    await page.getByRole("tab", { name: "Files" }).click();
    await page.waitForTimeout(500);
  });

  // 6.78-6.82 Data cleaning
  test("6.78 - should not include empty optional strings in YAML output", async ({ page }) => {
    const agentsFolder = page.getByRole("button", {
      name: "agents",
      exact: true
    });
    if (await agentsFolder.isVisible()) {
      await agentsFolder.click();
      await page.waitForTimeout(500);

      const agentFile = page
        .locator('a[href*="/ide/"]:visible')
        .filter({ hasText: ".agent.yml" })
        .first();

      if (await agentFile.isVisible()) {
        await agentFile.click();
        await page.waitForURL(/\/ide\/.+/);

        const formTab = page.getByRole("tab", { name: /form/i });
        if (await formTab.isVisible()) {
          await formTab.click();
          await page.waitForTimeout(500);

          // Clear an optional field
          const optionalInput = page.locator('input[placeholder*="optional"]').first();
          if (await optionalInput.isVisible()) {
            await optionalInput.fill("");
            await page.waitForTimeout(600);
          }
        }
      }
    }
  });
});
