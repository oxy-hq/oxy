import { useQuery } from "@tanstack/react-query";

import queryKeys from "./queryKey";
import { service } from "@/services/service";

export default function useFile(
  pathb64: string,
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = true,
) {
  return useQuery({
    queryKey: queryKeys.file.get(pathb64),
    queryFn: () => service.getFile(pathb64),
    enabled,
    refetchOnWindowFocus: refetchOnWindowFocus,
    refetchOnMount,
  });
}
