import axios from "axios";

import { apiBaseURL } from "./env";

export const apiClient = axios.create({
  baseURL: apiBaseURL,
});
