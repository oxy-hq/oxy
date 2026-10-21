import { useMutation } from "@tanstack/react-query";
import { CliAuthService } from "@/services/api/apiToken";

/**
 * Decline a device-code request, so oxyc stops at its next poll instead of waiting out the ten
 * minutes. The page says it was declined either way: the only cost of a failure here is that
 * oxyc waits.
 */
const useDenyCliDevice = (userCode: string) =>
  useMutation<void, Error, void>({
    mutationFn: () => CliAuthService.denyDevice(userCode)
  });

export default useDenyCliDevice;
