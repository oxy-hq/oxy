import { useQuery } from "@tanstack/react-query";

import queryKeys from "../queryKey";
import { AppService } from "@/services/api";

export default function useApp(
  appPath64: string,
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false,
) {
  return useQuery({
    queryKey: queryKeys.app.get(appPath64),
    queryFn: () => AppService.getApp(appPath64),
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
    queryFn: () => AppService.getData(filePath),
    enabled,
    refetchOnWindowFocus: refetchOnWindowFocus,
    refetchOnMount,
  });
}
