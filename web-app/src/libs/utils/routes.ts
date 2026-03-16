const ROUTES = {
  ROOT: "/",

  AUTH: {
    LOGIN: "/login",
    GOOGLE_CALLBACK: "/auth/google/callback",
    OKTA_CALLBACK: "/auth/okta/callback",
    MAGIC_LINK_CALLBACK: "/auth/magic-link/callback"
  },
  GITHUB: {
    CALLBACK: "/github/callback"
  },
  SETTINGS: {
    GITHUB: "/github-settings",
    SECRETS_SETUP: "/secrets/setup"
  },

  WORKSPACE: {
    ROOT: "/workspaces",
    CREATE_WORKSPACE: "/create-workspace"
  },

  PROJECT: (projectId: string) => {
    let base = `/projects/${projectId}`;
    if (projectId === "00000000-0000-0000-0000-000000000000") {
      base = ``;
    }

    return {
      ROOT: base,
      HOME: `${base}/home`,
      NEW: `${base}/new`,
      REQUIRED_SECRETS: `${base}/settings/secrets`,

      THREADS: `${base}/threads`,
      THREAD: (threadId: string) => `${base}/threads/${threadId}`,

      WORKFLOW: (pathb64: string) => {
        const wfBase = `${base}/workflows/${pathb64}`;
        return {
          ROOT: wfBase
        };
      },

      APP: (pathb64: string) => `${base}/apps/${pathb64}`,

      IDE: {
        ROOT: `${base}/ide`,
        FILES: {
          ROOT: `${base}/ide/files`,
          FILE: (pathb64: string) => `${base}/ide/files/${pathb64}`,
          LOOKER_EXPLORE: (integrationName: string, model: string, exploreName: string) =>
            `${base}/ide/files/looker/${encodeURIComponent(integrationName)}/${encodeURIComponent(model)}/${encodeURIComponent(exploreName)}`
        },
        DATABASE: {
          ROOT: `${base}/ide/database`
        },
        SETTINGS: {
          ROOT: `${base}/ide/settings`,
          DATABASES: `${base}/ide/settings/databases`,
          ACTIVITY_LOGS: `${base}/ide/settings/activity-logs`,
          API_KEYS: `${base}/ide/settings/api-keys`
        },
        TESTS: {
          ROOT: `${base}/ide/tests`,
          RUNS: `${base}/ide/tests/runs`,
          TEST_FILE: (pathb64: string) => `${base}/ide/tests/${pathb64}`
        },
        OBSERVABILITY: {
          ROOT: `${base}/ide/observability`,
          TRACES: `${base}/ide/observability/traces`,
          TRACE: (traceId: string) => `${base}/ide/observability/traces/${traceId}`,
          CLUSTERS: `${base}/ide/observability/clusters`,
          CLUSTERS_V2: `${base}/ide/observability/clusters-v2`,
          METRICS: `${base}/ide/observability/metrics`,
          METRIC: (metricName: string) =>
            `${base}/ide/observability/metrics/${encodeURIComponent(metricName)}`,
          EXECUTION_ANALYTICS: `${base}/ide/observability/execution-analytics`
        }
      },

      CONTEXT_GRAPH: `${base}/context-graph`
    };
  }
} as const;

export default ROUTES;
