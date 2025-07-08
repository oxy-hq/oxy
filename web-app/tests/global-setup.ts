import { test as setup } from "@playwright/test";

/**
 * Global setup that runs before all tests.
 * Use this for any global configuration that needs to happen once.
 */
setup("global setup", async () => {
  // Add any global setup logic here
  // For example: setting up test database, clearing caches, etc.
  console.log("Running global test setup...");
});
