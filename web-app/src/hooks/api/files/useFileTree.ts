import { useQuery } from "@tanstack/react-query";

import { service } from "@/services/service";

export default function useFileTree(
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false,
) {
  return useQuery({
    queryKey: ["fileTree"],
    queryFn: service.getFileTree,
    enabled,
    refetchOnWindowFocus: refetchOnWindowFocus,
    refetchOnMount,
  });
}
