import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { CustomAppsService } from "@/services/api/customApps";
import { errMessage } from "../errMessage";
import queryKeys from "../queryKey";

export const useDeleteApp = () => {
  const qc = useQueryClient();
  return useMutation<void, Error, string>({
    mutationFn: CustomAppsService.delete,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.customApps.all() });
    },
    // Without this a refused delete (e.g. the OLTP-store guard's 409) was a
    // silent no-op — the spinner stopped, the app stayed, no toast.
    onError: (err) => toast.error(errMessage(err, "Failed to delete"))
  });
};
