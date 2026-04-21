import { fetchEventSource } from "@microsoft/fetch-event-source";
import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef } from "react";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { CoordinatorService } from "@/services/api/coordinator";
import queryKeys from "../queryKey";

/**
 * Subscribes to the coordinator live SSE stream.
 * When a snapshot event arrives, invalidates the active runs query
 * so the UI refreshes automatically.
 */
const useCoordinatorLive = () => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  const queryClient = useQueryClient();
  const abortRef = useRef<AbortController | null>(null);

  useEffect(() => {
    const controller = new AbortController();
    abortRef.current = controller;
    const token = localStorage.getItem("auth_token");

    fetchEventSource(CoordinatorService.liveStreamUrl(projectId), {
      method: "GET",
      headers: {
        Authorization: token ?? ""
      },
      openWhenHidden: true,
      signal: controller.signal,
      onmessage(ev) {
        if (ev.event === "snapshot") {
          queryClient.invalidateQueries({
            queryKey: queryKeys.coordinator.activeRuns(projectId)
          });
        }
      },
      onerror() {
        // Silently reconnect on error — fetchEventSource handles retries.
      }
    });

    return () => {
      controller.abort();
    };
  }, [projectId, queryClient]);
};

export default useCoordinatorLive;
