import { useQuery } from "@tanstack/react-query";
import { apiStatus } from "@/libs/apiError";
import { CliAuthService } from "@/services/api/apiToken";
import type { CliDeviceRequest } from "@/types/apiToken";
import queryKeys from "../queryKey";

/**
 * The device-code request a user code names, for `/cli-auth/device`. Read once: the page acts
 * on what it showed, and a request that has been decided answers 404, so a refetch would only
 * swap the approval for "no such request" under the person's cursor. A 404 is an answer, not a
 * failure to retry.
 */
const useCliDeviceRequest = (userCode: string | null) =>
  useQuery<CliDeviceRequest, Error>({
    queryKey: queryKeys.cliDevice.request(userCode ?? ""),
    queryFn: () => CliAuthService.deviceRequest(userCode ?? ""),
    enabled: Boolean(userCode),
    staleTime: Number.POSITIVE_INFINITY,
    refetchOnWindowFocus: false,
    refetchOnReconnect: false,
    retry: (failures, error) => apiStatus(error) !== 404 && failures < 2
  });

export default useCliDeviceRequest;
