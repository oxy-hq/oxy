import axios from "axios";
import { toast } from "sonner";

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
  baseURL: apiBaseURL
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
      localStorage.removeItem("auth_token");
      localStorage.removeItem("user");
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

    return Promise.reject(error);
  };
};

apiClient.interceptors.response.use((response) => response, makeResponseErrorHandler());

export const vibeCodingClient = axios.create({
  baseURL: "http://localhost:8000"
});
