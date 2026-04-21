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

  INVITE: (token: string) => `/invite/${token}`,

  // Org-scoped routes. Passing an empty `orgSlug` degrades to flat local-mode
  // paths (e.g. `/home` instead of `/acme/workspaces/<id>/home`) — in local
  // mode there's no org and a single implicit workspace, and every call site
  // already defaults to `useCurrentOrg(...).slug ?? ""`, so this makes URL
  // building mode-agnostic without touching the ~100 consumers.
  ORG: (orgSlug: string) => {
    const isLocal = orgSlug === "";
    const base = isLocal ? "" : `/${orgSlug}`;
    return {
      ROOT: isLocal ? "/" : base,
      WORKSPACES: `${base}/workspaces`,
      MEMBERS: `${base}/members`,
      SETTINGS: `${base}/settings`,

      WORKSPACE: (wsId: string) => {
        const wsBase = isLocal ? "" : `${base}/workspaces/${wsId}`;
        return {
          ROOT: wsBase,
          HOME: `${wsBase}/home`,
          NEW: `${wsBase}/new`,
          REQUIRED_SECRETS: `${wsBase}/ide/settings/secrets`,

          THREADS: `${wsBase}/threads`,
          THREAD: (threadId: string) => `${wsBase}/threads/${threadId}`,

          WORKFLOW: (pathb64: string) => ({
            ROOT: `${wsBase}/workflows/${pathb64}`
          }),

          APP: (pathb64: string) => `${wsBase}/apps/${pathb64}`,

          IDE: {
            ROOT: `${wsBase}/ide`,
            FILES: {
              ROOT: `${wsBase}/ide/files`,
              FILE: (pathb64: string) => `${wsBase}/ide/files/${pathb64}`,
              LOOKER_EXPLORE: (integrationName: string, model: string, exploreName: string) =>
                `${wsBase}/ide/files/looker/${encodeURIComponent(integrationName)}/${encodeURIComponent(model)}/${encodeURIComponent(exploreName)}`
            },
            DATABASE: {
              ROOT: `${wsBase}/ide/database`
            },
            SETTINGS: {
              ROOT: `${wsBase}/ide/settings`,
              DATABASES: `${wsBase}/ide/settings/databases`,
              REPOSITORIES: `${wsBase}/ide/settings/repositories`,
              ACTIVITY_LOGS: `${wsBase}/ide/settings/activity-logs`,
              API_KEYS: `${wsBase}/ide/settings/api-keys`,
              SECRETS: `${wsBase}/ide/settings/secrets`,
              MEMBERS: `${wsBase}/ide/settings/members`
            },
            TESTS: {
              ROOT: `${wsBase}/ide/tests`,
              RUNS: `${wsBase}/ide/tests/runs`,
              TEST_FILE: (pathb64: string) => `${wsBase}/ide/tests/${pathb64}`
            },
            COORDINATOR: {
              ROOT: `${wsBase}/ide/coordinator`,
              ACTIVE_RUNS: `${wsBase}/ide/coordinator/active-runs`,
              RUN_HISTORY: `${wsBase}/ide/coordinator/run-history`,
              RECOVERY: `${wsBase}/ide/coordinator/recovery`,
              QUEUE: `${wsBase}/ide/coordinator/queue`,
              RUN_TREE: (runId: string) => `${wsBase}/ide/coordinator/runs/${runId}/tree`
            },
            OBSERVABILITY: {
              ROOT: `${wsBase}/ide/observability`,
              TRACES: `${wsBase}/ide/observability/traces`,
              TRACE: (traceId: string) => `${wsBase}/ide/observability/traces/${traceId}`,
              CLUSTERS: `${wsBase}/ide/observability/clusters`,
              CLUSTERS_V2: `${wsBase}/ide/observability/clusters-v2`,
              METRICS: `${wsBase}/ide/observability/metrics`,
              METRIC: (metricName: string) =>
                `${wsBase}/ide/observability/metrics/${encodeURIComponent(metricName)}`,
              EXECUTION_ANALYTICS: `${wsBase}/ide/observability/execution-analytics`
            }
          },

          CONTEXT_GRAPH: `${wsBase}/context-graph`
        };
      }
    };
  }
} as const;

export default ROUTES;
