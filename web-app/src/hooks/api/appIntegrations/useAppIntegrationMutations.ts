import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import {
  type AppIntegrationKind,
  AppIntegrationsService,
  type UpsertAppIntegrationBody
} from "@/services/api/appIntegrations";
import queryKeys from "../queryKey";

export function useUpsertAppIntegration() {
  const { project, branchName } = useCurrentProjectBranch();
  const queryClient = useQueryClient();

  return useMutation<void, Error, UpsertAppIntegrationBody>({
    mutationFn: (body) => AppIntegrationsService.upsert(project.id, branchName, body),
    onSuccess: (_, body) => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.appIntegrations.list(project.id, branchName)
      });
      toast.success(`${formatKindLabel(body.kind)} configuration saved`);
    },
    onError: (error, body) => {
      console.error(`Failed to save ${body.kind} integration:`, error);
      toast.error(`Failed to save ${formatKindLabel(body.kind)} configuration`, {
        description: error.message
      });
    }
  });
}

export function useDeleteAppIntegration() {
  const { project, branchName } = useCurrentProjectBranch();
  const queryClient = useQueryClient();

  return useMutation<void, Error, AppIntegrationKind>({
    mutationFn: (kind) => AppIntegrationsService.remove(project.id, branchName, kind),
    onSuccess: (_, kind) => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.appIntegrations.list(project.id, branchName)
      });
      toast.success(`${formatKindLabel(kind)} disconnected`);
    },
    onError: (error, kind) => {
      console.error(`Failed to remove ${kind} integration:`, error);
      toast.error(`Failed to disconnect ${formatKindLabel(kind)}`, {
        description: error.message
      });
    }
  });
}

function formatKindLabel(kind: AppIntegrationKind): string {
  switch (kind) {
    case "toast":
      return "Toast POS";
    case "openweathermap":
      return "OpenWeatherMap";
    case "besttime":
      return "BestTime";
    case "unifi":
      return "UniFi Cameras";
  }
}
