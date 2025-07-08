import { useQuery } from "@tanstack/react-query";
import { SecretService } from "@/services/secretService";
import { SecretListResponse } from "@/types/secret";
import queryKeys from "./queryKey";

const useSecrets = (
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false,
) =>
  useQuery<SecretListResponse, Error>({
    queryKey: queryKeys.secret.list(),
    queryFn: () => SecretService.listSecrets(),
    enabled,
    refetchOnWindowFocus,
    refetchOnMount,
  });

export default useSecrets;
