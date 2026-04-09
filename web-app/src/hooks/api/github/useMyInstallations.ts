import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import queryKeys from "@/hooks/api/queryKey";
import { apiClient } from "@/services/api/axios";
import type { OAuthInstallation } from "@/types/github";

export interface MyInstallationsResponse {
  installations: OAuthInstallation[];
  selection_token: string;
}

/** Fetches the user's GitHub App installations using their stored OAuth token.
 *  Disabled when `skip` is true.
 *  Returns `null` (not error) when no token is stored yet (404) — caller
 *  checks `data === null` to detect the no-token state. */
export const useMyInstallations = (skip: boolean) => {
  return useQuery<MyInstallationsResponse | null>({
    queryKey: queryKeys.github.myInstallations,
    queryFn: async () => {
      try {
        const response = await apiClient.get<MyInstallationsResponse>("/github/my-installations");
        return response.data;
      } catch (err) {
        // 404 = no stored OAuth token; resolve as null so callers can distinguish
        // "not fetched yet" (undefined) from "fetched but no token" (null).
        if (axios.isAxiosError(err) && err.response?.status === 404) {
          return null;
        }
        throw err;
      }
    },
    enabled: !skip,
    retry: false,
    staleTime: 30 * 1000
  });
};
