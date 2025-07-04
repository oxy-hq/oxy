import { DatabaseService } from "@/services/api";
import { useMutation } from "@tanstack/react-query";

export default function useExecuteSql() {
  return useMutation({
    mutationFn: (data: { pathb64: string; sql: string; database: string }) =>
      DatabaseService.executeSql(data.pathb64, data.sql, data.database),
  });
}
