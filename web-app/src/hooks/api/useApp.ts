import { useQuery } from "@tanstack/react-query";

import queryKeys from "./queryKey";
import { service } from "@/services/service";

export default function useApp(
  appPath64: string,
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false,
) {
  return useQuery({
    queryKey: queryKeys.app.get(appPath64),
    queryFn: () => service.getApp(appPath64),
    enabled,
    refetchOnWindowFocus: refetchOnWindowFocus,
    refetchOnMount,
  });
}

export function useData(
  filePath: string,
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false,
) {
  return useQuery({
    queryKey: queryKeys.app.getData(filePath),
    queryFn: () => service.getData(filePath),
    enabled,
    refetchOnWindowFocus: refetchOnWindowFocus,
    refetchOnMount,
  });
}
