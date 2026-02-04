import { defineConfig, devices } from "@playwright/test";

/**
 * Determine optimal number of workers
 * - If PLAYWRIGHT_WORKERS env var is set, use it
 * - On CI, use 2 workers
 * - Locally, use undefined (50% of CPU cores)
 */
const getWorkerCount = () => {
  if (process.env.PLAYWRIGHT_WORKERS) {
    return Number(process.env.PLAYWRIGHT_WORKERS);
  }
  return process.env.CI ? 2 : undefined;
};

/**
 * @see https://playwright.dev/docs/test-configuration
 */
export default defineConfig({
  testDir: "./tests/e2e",
  /* Global setup - runs once before all tests */
  globalSetup: "./tests/e2e/global-setup.ts",
  /* Run tests in files in parallel */
  fullyParallel: true,
  /* Fail the build on CI if you accidentally left test.only in the source code. */
  forbidOnly: false,
  /* Retry on CI only */
  retries: process.env.CI ? 2 : 0,
  /* Use optimal workers: 50% of CPU cores locally, fixed on CI */
  workers: getWorkerCount(),
  /* Reporter to use. See https://playwright.dev/docs/test-reporters */
  reporter: process.env.CI ? "github" : "html",
  /* Shared settings for all the projects below. See https://playwright.dev/docs/api/class-testoptions. */
  use: {
    /* Base URL to use in actions like `await page.goto('/')`. */
    baseURL: "http://localhost:5173",

    /* Collect trace when retrying the failed test. See https://playwright.dev/docs/trace-viewer */
    trace: "on-first-retry",
    screenshot: "only-on-failure",
    video: "off", // Disable video to speed up tests; enable only if debugging
    ignoreHTTPSErrors: true,

    /* Performance optimizations */
    navigationTimeout: 30000, // Increased from 15s to 30s to allow for backend initialization after database reset
    actionTimeout: 10000 // Faster action timeouts
  },
  testIgnore: "**/ide-files/**",

  timeout: 60000, // Reduced from 120s to 60s
  expect: {
    timeout: 5000
  },
  /* Configure projects for major browsers */
  projects: [
    {
      name: "chromium",
      use: {
        ...devices["Desktop Chrome"],
        // Run headless by default, unless --headed flag is used
        headless: !process.env.HEADED,
        launchOptions: {
          args: [
            "--disable-blink-features=AutomationControlled",
            "--disable-background-timer-throttling",
            "--disable-backgrounding-occluded-windows",
            "--disable-renderer-backgrounding",
            // Performance optimizations
            "--disable-dev-shm-usage", // Reduce shared memory usage
            "--disable-extensions", // Disable extensions
            "--disable-gpu", // Disable GPU hardware acceleration
            "--no-sandbox" // Disable sandboxing for faster startup
          ]
        }
      }
    }
  ],

  /* Run your local dev server before starting the tests */
  webServer: [
    {
      command: "pnpm run dev",
      url: "http://localhost:5173",
      reuseExistingServer: true,
      timeout: 120000 // 2 minutes max for server startup
    }
  ]
});
