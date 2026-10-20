import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { CustomAppsService } from "@/services/api/customApps";
import { errMessage } from "../errMessage";
import { isForbidden } from "../httpStatus";
import queryKeys from "../queryKey";

/**
 * The app's environments — production, staging, and every dev sandbox
 * (`GET /customer-apps/{id}/environments`).
 *
 * A deleting sandbox stays in the list as `status: "deleting"` until its teardown
 * lands, so the panel polls briefly while one is: a row that vanished on click and
 * a row that is still being torn down are different answers.
 *
 * A 403 — the caller may not open the app's non-production environments — is an
 * answer, not a fault, so it is not retried: the Secrets pane's picker and this
 * panel each say so as soon as it arrives.
 */
export const useAppEnvironments = (appId: string | undefined) =>
  useQuery({
    queryKey: queryKeys.customApps.environments(appId ?? ""),
    queryFn: () => CustomAppsService.listEnvironments(appId as string),
    enabled: !!appId,
    retry: (failures, error) => !isForbidden(error) && failures < 3,
    staleTime: 15_000,
    refetchInterval: (query) =>
      query.state.data?.some((e) => e.status === "deleting") ? 5_000 : false
  });

export const useCreateAppEnvironment = (appId: string) => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (name: string) => CustomAppsService.createEnvironment(appId, name),
    onSuccess: (env) => {
      qc.invalidateQueries({ queryKey: queryKeys.customApps.environments(appId) });
      toast.success(`Sandbox ${env.name} created`);
    },
    onError: (err) => toast.error(errMessage(err, "Couldn't create the sandbox"))
  });
};

export const useDeleteAppEnvironment = (appId: string) => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (name: string) => CustomAppsService.deleteEnvironment(appId, name),
    onSuccess: (accepted) => {
      qc.invalidateQueries({ queryKey: queryKeys.customApps.environments(appId) });
      // The sandbox's secrets go with it.
      qc.invalidateQueries({ queryKey: queryKeys.customApps.secrets(appId) });
      toast.success(`Tearing down ${accepted.name}`);
    },
    onError: (err) => toast.error(errMessage(err, "Couldn't delete the sandbox"))
  });
};
