import { useMutation } from "@tanstack/react-query";
import { service } from "@/services/service";

export function useDatabaseSync() {
  return useMutation({
    mutationFn: ({
      database,
      options,
    }: {
      database?: string;
      options?: { datasets?: string[] };
    }) => service.syncDatabase(database, options),
  });
}
