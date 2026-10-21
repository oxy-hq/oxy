import { useMutation, useQueryClient } from "@tanstack/react-query";
import { CliAuthService } from "@/services/api/apiToken";
import type { CliDeviceApproveRequest } from "@/types/apiToken";
import queryKeys from "../queryKey";
import { isUnavailableAppError } from "../userTokens/tokenErrors";

/**
 * Approve a device-code request (`oxyc … --no-browser`) under the session: the counterpart of
 * `useAuthorizeCli`, with nothing to hand back, since oxyc collects the token by polling. The
 * server checks the mint exactly as it checks `/auth/cli/authorize`, and a refusal there also
 * refuses the request, so oxyc stops with the same reason. No toast: the page renders it.
 */
const useApproveCliDevice = (userCode: string) => {
  const queryClient = useQueryClient();
  return useMutation<void, Error, CliDeviceApproveRequest>({
    mutationFn: (request) => CliAuthService.approveDevice(userCode, request),
    onError: (error) => {
      if (isUnavailableAppError(error)) {
        queryClient.invalidateQueries({ queryKey: queryKeys.userToken.options() });
      }
    }
  });
};

export default useApproveCliDevice;
