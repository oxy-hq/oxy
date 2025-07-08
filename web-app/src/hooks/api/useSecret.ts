import { useQuery } from "@tanstack/react-query";
import { SecretService } from "@/services/secretService";
import { Secret } from "@/types/secret";
import queryKeys from "./queryKey";

const useSecret = (
  id: string,
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false,
) =>
  useQuery<Secret, Error>({
    queryKey: queryKeys.secret.item(id),
    queryFn: () => SecretService.getSecret(id),
    enabled: enabled && !!id,
    refetchOnWindowFocus,
    refetchOnMount,
  });

export default useSecret;
