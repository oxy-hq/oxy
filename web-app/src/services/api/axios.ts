import axios from "axios";
import { toast } from "sonner";

import { clearAuthScopedStorage } from "@/libs/utils/authStorage";
import { reportIdeReachable, reportIdeUnavailable } from "@/libs/utils/ideHealth";
import { usePaywallStore } from "@/stores/usePaywallStore";
import { apiBaseURL } from "../env";

const publicAPIPaths = [
  "/auth/google",
  "/auth/okta",
  "/auth/config",
  "/auth/magic-link/request",
  "/auth/magic-link/verify",
  "/health",
  "/ready",
  "/live"
];

export const apiClient = axios.create({
  baseURL: apiBaseURL,
  // Send + receive cookies on every request. Without this, the browser
  // drops the `Set-Cookie: oxy_session=...` header on the login response,
  // and the customer-app iframe + cross-subdomain SSO have nothing to
  // attach. The bearer token in localStorage continues to work for the
  // SPA itself; the cookie is what lets bundle traffic authenticate.
  withCredentials: true
});

apiClient.interceptors.request.use(
  (config) => {
    const token = localStorage.getItem("auth_token");
    if (token) {
      config.headers.Authorization = token;
    }
    return config;
  },
  (error) => {
    return Promise.reject(error);
  }
);

// Flight-dedupe 403 toasts: bursts of parallel mutations shouldn't stack
// multiple identical permission-denied toasts.
const makeResponseErrorHandler = () => {
  let last403At = 0;
  return (error: {
    response?: {
      status?: number;
      headers?: Record<string, string | undefined>;
      data?: {
        code?: string;
        status?: "incomplete" | "unpaid" | "canceled";
        contact_required?: boolean;
      };
    };
    config?: { url?: string };
  }) => {
    const status = error.response?.status;
    const url = error.config?.url ?? "";

    if (status === 401 && !publicAPIPaths.includes(url)) {
      // Sweep persisted per-user state alongside the token; a hard navigate
      // to /login lets Zustand rehydrate the next user from a clean slate.
      clearAuthScopedStorage();
      window.location.href = "/login";
    }

    if (status === 403 && !publicAPIPaths.includes(url)) {
      const now = Date.now();
      if (now - last403At > 1500) {
        last403At = now;
        toast.error("You don't have permission to do this.");
      }
    }

    if (status === 402 && error.response?.data?.code === "subscription_required") {
      const billingStatus = error.response.data.status ?? "incomplete";
      const contactRequired = error.response.data.contact_required !== false;
      usePaywallStore.getState().show(billingStatus, contactRequired);
    }

    // The developer-environment singleton is unreachable: the serving replica
    // stamps `x-oxy-required-role: ide` on its 502 so this is distinguishable
    // from a generic gateway error. Surface the global IDE-unavailable banner.
    if (status === 502 && error.response?.headers?.["x-oxy-required-role"] === "ide") {
      reportIdeUnavailable(url);
    }

    return Promise.reject(error);
  };
};

apiClient.interceptors.response.use((response) => {
  // A response served by the IDE backend — directly, or forwarded to it by a
  // serve replica — proves it's reachable again, so retire any IDE-down banner.
  // The header is `<role>@<host>#<pid>` (e.g. `ide@box#42`), so match on the
  // role prefix, not the whole value.
  const servedBy = (response.headers as Record<string, string | undefined> | undefined)?.[
    "x-oxy-served-by"
  ];
  if (servedBy?.split("@")[0] === "ide") reportIdeReachable();
  return response;
}, makeResponseErrorHandler());
