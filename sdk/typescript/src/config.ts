/**
 * Configuration for the Oxy SDK
 */
export interface OxyConfig {
  /**
   * Base URL of the Oxy API (e.g., 'https://api.oxy.tech' or 'http://localhost:3000')
   */
  baseUrl: string;

  /**
   * API key for authentication (optional for local development)
   */
  apiKey?: string;

  /**
   * Project ID (UUID)
   */
  projectId: string;

  /**
   * Optional branch name (defaults to current branch if not specified)
   */
  branch?: string;

  /**
   * Request timeout in milliseconds (default: 30000)
   */
  timeout?: number;

  /**
   * Parent window origin for postMessage authentication (iframe scenarios)
   * Required when using postMessage auth for security.
   * Example: 'https://app.example.com'
   * Use '*' only in development!
   */
  parentOrigin?: string;

  /**
   * Disable automatic postMessage authentication even if in iframe
   * Set to true if you want to provide API key manually in iframe context
   */
  disableAutoAuth?: boolean;
}

/**
 * Safely get environment variable in both Node.js and browser environments
 */
function getEnvVar(name: string): string | undefined {
  // Check if we're in Node.js
  if (typeof process !== "undefined" && process.env) {
    return process.env[name];
  }

  // Check if we're in a Vite environment (browser)
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  if (typeof import.meta !== "undefined" && (import.meta as any).env) {
    // Try with VITE_ prefix first (Vite convention)
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const viteValue = (import.meta as any).env[`VITE_${name}`];
    if (viteValue !== undefined) return viteValue;

    // Try without prefix
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    return (import.meta as any).env[name];
  }

  return undefined;
}

/**
 * Creates an Oxy configuration from environment variables
 *
 * Environment variables:
 * - OXY_URL: Base URL of the Oxy API
 * - OXY_API_KEY: API key for authentication
 * - OXY_PROJECT_ID: Project ID (UUID)
 * - OXY_BRANCH: (Optional) Branch name
 *
 * @param overrides - Optional configuration overrides
 * @returns OxyConfig object
 * @throws Error if required environment variables are missing
 */
export function createConfig(overrides?: Partial<OxyConfig>): OxyConfig {
  const baseUrl = overrides?.baseUrl || getEnvVar("OXY_URL");
  const apiKey = overrides?.apiKey || getEnvVar("OXY_API_KEY");
  const projectId = overrides?.projectId || getEnvVar("OXY_PROJECT_ID");

  if (!baseUrl) {
    throw new Error(
      "OXY_URL environment variable or baseUrl config is required",
    );
  }

  if (!projectId) {
    throw new Error(
      "OXY_PROJECT_ID environment variable or projectId config is required",
    );
  }

  return {
    baseUrl: baseUrl.replace(/\/$/, ""), // Remove trailing slash
    apiKey,
    projectId,
    branch: overrides?.branch || getEnvVar("OXY_BRANCH"),
    timeout: overrides?.timeout || 30000,
    parentOrigin: overrides?.parentOrigin,
    disableAutoAuth: overrides?.disableAutoAuth,
  };
}

/**
 * Creates an Oxy configuration asynchronously with support for postMessage authentication
 *
 * This is the recommended method for iframe scenarios where authentication
 * needs to be obtained from the parent window via postMessage.
 *
 * When running in an iframe without an API key, this function will:
 * 1. Detect the iframe context
 * 2. Send an authentication request to the parent window
 * 3. Wait for the parent to respond with credentials
 * 4. Return the configured client
 *
 * Environment variables (fallback):
 * - OXY_URL: Base URL of the Oxy API
 * - OXY_API_KEY: API key for authentication
 * - OXY_PROJECT_ID: Project ID (UUID)
 * - OXY_BRANCH: (Optional) Branch name
 *
 * @param overrides - Optional configuration overrides
 * @returns Promise resolving to OxyConfig object
 * @throws Error if required configuration is missing
 * @throws PostMessageAuthTimeoutError if parent doesn't respond
 *
 * @example
 * ```typescript
 * // Automatic iframe detection and authentication
 * const config = await createConfigAsync({
 *   parentOrigin: 'https://app.example.com',
 *   projectId: 'my-project-id',
 *   baseUrl: 'https://api.oxy.tech'
 * });
 * ```
 */
export async function createConfigAsync(
  overrides?: Partial<OxyConfig>,
): Promise<OxyConfig> {
  // Import postMessage utilities (dynamic to avoid circular deps)
  const { isInIframe } = await import("./auth/postMessage");

  // Start with environment variables and overrides
  let baseUrl = overrides?.baseUrl || getEnvVar("OXY_URL");
  let apiKey = overrides?.apiKey || getEnvVar("OXY_API_KEY");
  let projectId = overrides?.projectId || getEnvVar("OXY_PROJECT_ID");

  const disableAutoAuth = overrides?.disableAutoAuth ?? false;
  const parentOrigin =
    overrides?.parentOrigin ||
    (window?.location.ancestorOrigins?.[0]
      ? window.location.ancestorOrigins[0]
      : "https://app.oxy.tech");
  // Automatic iframe detection and authentication
  if (!disableAutoAuth && isInIframe() && !apiKey) {
    if (!parentOrigin) {
      logWarningAboutMissingParentOrigin();
    } else {
      apiKey = await attemptPostMessageAuth(
        parentOrigin,
        overrides?.timeout || 5000,
        apiKey,
        projectId,
        baseUrl,
      )
        .then((result) => {
          if (result.projectId) projectId = result.projectId;
          if (result.baseUrl) baseUrl = result.baseUrl;
          return result.apiKey;
        })
        .catch((error) => {
          console.error(
            "[Oxy SDK] Failed to authenticate via postMessage:",
            (error as Error).message,
          );
          return apiKey;
        });
    }
  }

  return createFinalConfig(baseUrl, apiKey, projectId, overrides);
}

function logWarningAboutMissingParentOrigin(): void {
  console.warn(
    "[Oxy SDK] Running in iframe without API key and no parentOrigin specified. " +
      "PostMessage authentication will be skipped. " +
      "Provide parentOrigin config to enable automatic authentication.",
  );
}

async function attemptPostMessageAuth(
  parentOrigin: string,
  timeout: number,
  currentApiKey: string | undefined,
  currentProjectId: string | undefined,
  currentBaseUrl: string | undefined,
): Promise<{ apiKey?: string; projectId?: string; baseUrl?: string }> {
  const { requestAuthFromParent } = await import("./auth/postMessage");
  const authResult = await requestAuthFromParent({ parentOrigin, timeout });

  console.log("[Oxy SDK] Successfully authenticated via postMessage");

  return {
    apiKey: authResult.apiKey || currentApiKey,
    projectId: authResult.projectId || currentProjectId,
    baseUrl: authResult.baseUrl || currentBaseUrl,
  };
}

function createFinalConfig(
  baseUrl: string | undefined,
  apiKey: string | undefined,
  projectId: string | undefined,
  overrides?: Partial<OxyConfig>,
): OxyConfig {
  // Validation
  if (!baseUrl) {
    throw new Error(
      "OXY_URL environment variable or baseUrl config is required",
    );
  }

  if (!projectId) {
    throw new Error(
      "OXY_PROJECT_ID environment variable or projectId config is required",
    );
  }

  return {
    baseUrl: baseUrl.replace(/\/$/, ""), // Remove trailing slash
    apiKey,
    projectId,
    branch: overrides?.branch || getEnvVar("OXY_BRANCH"),
    timeout: overrides?.timeout || 30000,
    parentOrigin: overrides?.parentOrigin,
    disableAutoAuth: overrides?.disableAutoAuth,
  };
}
