const ROUTES = {
  ROOT: "/",

  AUTH: {
    LOGIN: "/login",
    GOOGLE_CALLBACK: "/auth/google/callback",
    OKTA_CALLBACK: "/auth/okta/callback",
    MAGIC_LINK_CALLBACK: "/auth/magic-link/callback",
    GITHUB_AUTH_CALLBACK: "/auth/github/callback"
  },
  GITHUB: {
    CALLBACK: "/github/callback"
  },
  SETUP: "/setup",
  WORKSPACES: "/workspaces",
  MEMBERS: "/members",

  WORKSPACE: (_workspaceId: string) => {
    // Navigation routes are flat (no workspace-ID prefix) — workspace context is held
    // in the Zustand store. API calls use the ID from useCurrentWorkspace().
    // Exception: IDE and shareable resource URLs include the workspace ID so they
    // are workspace-locked and can be bookmarked / shared as deep links.
    const base = ``;
    const ideBase = `/workspaces/${_workspaceId}`;

    return {
      ROOT: base || "/",
      HOME: `${base}/home`,
      NEW: `${base}/new`,
      REQUIRED_SECRETS: `${ideBase}/ide/settings/secrets`,

      THREADS: `${base}/threads`,
      // Thread and workflow URLs include the workspace ID so they can be shared as deep links.
      THREAD: (threadId: string) => `/workspaces/${_workspaceId}/threads/${threadId}`,

      WORKFLOW: (pathb64: string) => {
        return {
          ROOT: `/workspaces/${_workspaceId}/workflows/${pathb64}`
        };
      },

      APP: (pathb64: string) => `/workspaces/${_workspaceId}/apps/${pathb64}`,

      IDE: {
        ROOT: `${ideBase}/ide`,
        FILES: {
          ROOT: `${ideBase}/ide/files`,
          FILE: (pathb64: string) => `${ideBase}/ide/files/${pathb64}`,
          LOOKER_EXPLORE: (integrationName: string, model: string, exploreName: string) =>
            `${ideBase}/ide/files/looker/${encodeURIComponent(integrationName)}/${encodeURIComponent(model)}/${encodeURIComponent(exploreName)}`
        },
        DATABASE: {
          ROOT: `${ideBase}/ide/database`
        },
        SETTINGS: {
          ROOT: `${ideBase}/ide/settings`,
          DATABASES: `${ideBase}/ide/settings/databases`,
          REPOSITORIES: `${ideBase}/ide/settings/repositories`,
          ACTIVITY_LOGS: `${ideBase}/ide/settings/activity-logs`,
          API_KEYS: `${ideBase}/ide/settings/api-keys`,
          SECRETS: `${ideBase}/ide/settings/secrets`
        },
        TESTS: {
          ROOT: `${ideBase}/ide/tests`,
          RUNS: `${ideBase}/ide/tests/runs`,
          TEST_FILE: (pathb64: string) => `${ideBase}/ide/tests/${pathb64}`
        },
        OBSERVABILITY: {
          ROOT: `${ideBase}/ide/observability`,
          TRACES: `${ideBase}/ide/observability/traces`,
          TRACE: (traceId: string) => `${ideBase}/ide/observability/traces/${traceId}`,
          CLUSTERS: `${ideBase}/ide/observability/clusters`,
          CLUSTERS_V2: `${ideBase}/ide/observability/clusters-v2`,
          METRICS: `${ideBase}/ide/observability/metrics`,
          METRIC: (metricName: string) =>
            `${ideBase}/ide/observability/metrics/${encodeURIComponent(metricName)}`,
          EXECUTION_ANALYTICS: `${ideBase}/ide/observability/execution-analytics`
        }
      },

      CONTEXT_GRAPH: `${ideBase}/context-graph`
    };
  }
} as const;

export default ROUTES;
