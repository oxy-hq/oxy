import axios from "axios";

import { apiBaseURL } from "../env";

const publicAPIPaths = [
  "/auth/login",
  "/auth/register",
  "/auth/verify-email",
  "/auth/google",
  "/auth/okta",
  "/auth/config",
];

export const apiClient = axios.create({
  baseURL: apiBaseURL,
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
  },
);

apiClient.interceptors.response.use(
  (response) => response,
  (error) => {
    if (
      error.response?.status === 401 &&
      !publicAPIPaths.includes(error.config.url)
    ) {
      localStorage.removeItem("auth_token");
      localStorage.removeItem("user");
      window.location.href = "/login";
    }
    return Promise.reject(error);
  },
);

export const vibeCodingClient = axios.create({
  baseURL: "http://localhost:8000",
});
