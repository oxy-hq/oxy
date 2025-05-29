import { useQuery } from "@tanstack/react-query";
import { DatabaseInfo } from "@/types/database";

import queryKeys from "./queryKey";
import { service } from "@/services/service";

export default function useDatabases(
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false,
) {
  return useQuery<DatabaseInfo[], unknown>({
    queryKey: queryKeys.database.list(),
    queryFn: service.listDatabases,
    enabled,
    refetchOnWindowFocus: refetchOnWindowFocus,
    refetchOnMount,
  });
}
