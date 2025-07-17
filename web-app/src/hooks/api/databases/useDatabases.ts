import { useQuery } from "@tanstack/react-query";
import { DatabaseInfo } from "@/types/database";

import queryKeys from "../queryKey";
import { DatabaseService } from "@/services/api";

export default function useDatabases(
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false,
) {
  return useQuery<DatabaseInfo[], Error>({
    queryKey: queryKeys.database.list(),
    queryFn: DatabaseService.listDatabases,
    enabled,
    refetchOnWindowFocus: refetchOnWindowFocus,
    refetchOnMount,
  });
}
