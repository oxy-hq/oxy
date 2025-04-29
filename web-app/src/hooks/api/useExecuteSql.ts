import { service } from "@/services/service";
import { useMutation } from "@tanstack/react-query";

export default function useExecuteSql() {
  return useMutation({
    mutationFn: (data: { pathb64: string; sql: string; database: string }) =>
      service.executeSql(data.pathb64, data.sql, data.database),
  });
}
