import { useQuery } from "@tanstack/react-query";

import queryKeys from "./queryKey";
import { service } from "@/services/service";

export default function useApp(
  appPath: string,
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false,
) {
  return useQuery({
    queryKey: queryKeys.app.get(appPath),
    queryFn: () => service.getApp(appPath),
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

