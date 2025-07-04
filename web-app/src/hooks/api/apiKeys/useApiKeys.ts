import { useQuery } from "@tanstack/react-query";
import { ApiKeyService } from "@/services/api/apiKey";
import { ApiKeyListResponse } from "@/types/apiKey";
import queryKeys from "../queryKey";

const useApiKeys = (
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false,
) =>
  useQuery<ApiKeyListResponse, Error>({
    queryKey: queryKeys.apiKey.list(),
    queryFn: () => ApiKeyService.listApiKeys(),
    enabled,
    refetchOnWindowFocus,
    refetchOnMount,
  });

export default useApiKeys;
