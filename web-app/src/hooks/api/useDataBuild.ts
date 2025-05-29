import { useMutation } from "@tanstack/react-query";
import { service } from "@/services/service";

export function useDataBuild() {
  return useMutation({
    mutationFn: () => service.buildDatabase(),
  });
}
