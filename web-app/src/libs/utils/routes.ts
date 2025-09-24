const ROUTES = {
  ROOT: "/",

  AUTH: {
    LOGIN: "/login",
    REGISTER: "/register",
    VERIFY_EMAIL: "/verify-email",
    GOOGLE_CALLBACK: "/auth/google/callback",
  },
  SETTINGS: {
    GITHUB: "/github-settings",
    SECRETS_SETUP: "/secrets/setup",
  },

  ORG: {
    ROOT: "/workspaces",
  },

  PROJECT: (projectId: string) => {
    const base = `/projects/${projectId}`;
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
          ROOT: wfBase,
          RUN: (runId: string) => `${wfBase}/runs/${runId}`,
        };
      },

      APP: (pathb64: string) => `${base}/apps/${pathb64}`,

      IDE: {
        ROOT: `${base}/ide`,
        FILE: (pathb64: string) => `${base}/ide/${pathb64}`,
      },
    };
  },
} as const;

export default ROUTES;
