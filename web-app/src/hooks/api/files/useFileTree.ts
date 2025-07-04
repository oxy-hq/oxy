import { useQuery } from "@tanstack/react-query";

import { FileService } from "@/services/api";

export default function useFileTree(
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false,
) {
  return useQuery({
    queryKey: ["fileTree"],
    queryFn: FileService.getFileTree,
    enabled,
    refetchOnWindowFocus: refetchOnWindowFocus,
    refetchOnMount,
  });
}
