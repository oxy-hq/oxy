import axios from "axios";

import { apiBaseURL } from "./env";

export const apiClient = axios.create({
  baseURL: apiBaseURL,
});

export const vibeCodingClient = axios.create({
  baseURL: "https://localhost:8000",
});
