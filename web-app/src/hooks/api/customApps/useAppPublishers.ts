import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { CustomAppsService, type RegisterPublisherRequest } from "@/services/api/customApps";
import { errMessage } from "../errMessage";
import queryKeys from "../queryKey";

/** The GitHub workflows trusted to OIDC-publish this app. */
export const useAppPublishers = (appId: string | undefined) =>
  useQuery({
    queryKey: queryKeys.customApps.publishers(appId ?? ""),
    queryFn: () => CustomAppsService.listPublishers(appId as string),
    enabled: !!appId,
    staleTime: 30_000
  });

export const useRegisterAppPublisher = (appId: string) => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: RegisterPublisherRequest) =>
      CustomAppsService.registerPublisher(appId, body),
    onSuccess: (p) => {
      qc.invalidateQueries({ queryKey: queryKeys.customApps.publishers(appId) });
      toast.success(`${p.repo_owner}/${p.repo_name} can publish`);
    },
    onError: (err) =>
      toast.error(
        status(err) === 409
          ? "That workflow is already registered"
          : errMessage(err, "Couldn't register the publisher")
      )
  });
};

export const useDeleteAppPublisher = (appId: string) => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (publisherId: string) => CustomAppsService.deletePublisher(appId, publisherId),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.customApps.publishers(appId) });
      toast.success("Publisher removed");
    },
    onError: (err) => toast.error(errMessage(err, "Couldn't remove the publisher"))
  });
};

const status = (e: unknown): number | undefined =>
  (e as { response?: { status?: number } } | null)?.response?.status;
