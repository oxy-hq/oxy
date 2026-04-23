import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useRef } from "react";
import { useAcceptInvitation } from "@/hooks/api/organizations";
import queryKeys from "@/hooks/api/queryKey";
import type { InviteStatus } from "./types";

type AcceptInviteFlowArgs = {
  token: string | undefined;
  enabled: boolean;
  onSuccess: (slug: string) => void;
  onError: (status: InviteStatus) => void;
  onReset: () => void;
};

export function useAcceptInviteFlow({
  token,
  enabled,
  onSuccess,
  onError,
  onReset
}: AcceptInviteFlowArgs) {
  const queryClient = useQueryClient();
  const { mutateAsync } = useAcceptInvitation();
  const triggered = useRef(false);

  useEffect(() => {
    if (!enabled || !token || triggered.current) return;
    triggered.current = true;

    mutateAsync(token)
      .then(async (org) => {
        await queryClient.refetchQueries({ queryKey: queryKeys.org.list() });
        onSuccess(org.slug);
      })
      .catch((err) => onError(getErrorStatus(err)));
  }, [enabled, token, mutateAsync, queryClient, onSuccess, onError]);

  return useCallback(() => {
    triggered.current = false;
    onReset();
  }, [onReset]);
}

function getErrorStatus(err: unknown): InviteStatus {
  if (err && typeof err === "object" && "status" in err) {
    const { status } = err as { status: unknown };
    if (typeof status === "number") return status;
    if (status === "network") return "network";
  }
  return "unknown";
}
