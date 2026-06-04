import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { isAxiosError } from "axios";
import { toast } from "sonner";
import { AppAdminsService } from "@/services/api/access";
import queryKeys from "../queryKey";

export const useAppAdmins = () =>
  useQuery({
    queryKey: queryKeys.appAdmins.list(),
    queryFn: AppAdminsService.list
  });

export const useCreateAppAdmin = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (email: string) => AppAdminsService.create(email),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.appAdmins.list() });
      toast.success("Global Admin added");
    },
    onError: (err) => {
      const message = isAxiosError(err)
        ? (err.response?.data?.message ?? err.message)
        : err instanceof Error
          ? err.message
          : "Failed to add app admin";
      toast.error(message);
    }
  });
};

export const useRemoveAppAdmin = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => AppAdminsService.remove(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.appAdmins.list() });
      toast.success("Global Admin removed");
    },
    onError: (err) => {
      const message = isAxiosError(err)
        ? (err.response?.data?.message ?? err.message)
        : err instanceof Error
          ? err.message
          : "Failed to remove app admin";
      toast.error(message);
    }
  });
};
