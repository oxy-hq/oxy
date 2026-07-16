import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { AuditService } from "@/services/api/audit";
import type { AuditSearchParams } from "@/types/audit";
import queryKeys from "../queryKey";

/** Platform audit search. Keeps prior rows while refetching so filtering
 *  doesn't flash an empty table. */
export const useAuditSearch = (params: AuditSearchParams) =>
  useQuery({
    queryKey: queryKeys.audit.search(params as Record<string, unknown>),
    queryFn: () => AuditService.search(params),
    placeholderData: keepPreviousData
  });
