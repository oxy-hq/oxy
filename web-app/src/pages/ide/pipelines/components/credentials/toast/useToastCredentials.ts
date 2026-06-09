import { useState } from "react";
import { useCreateSecret } from "@/hooks/api/secrets/useSecretMutations";

export interface ToastScaffoldConfig {
  clientId: string;
  clientSecretVar: string;
  restaurantGuids: string[];
  baseUrl?: string;
}

const parseGuids = (raw: string): string[] =>
  raw
    .split(/[\s,]+/)
    .map((g) => g.trim())
    .filter(Boolean);

/** Owns the Toast credential form state, validation, secret persistence
 *  and scaffold-config shaping for the New Pipeline wizard. */
export function useToastCredentials() {
  const [clientId, setClientId] = useState("");
  const [clientSecret, setClientSecret] = useState("");
  const [secretName, setSecretName] = useState("");
  const [guids, setGuids] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const createSecret = useCreateSecret();

  const reset = () => {
    setClientId("");
    setClientSecret("");
    setSecretName("");
    setGuids("");
    setBaseUrl("");
  };

  /** Returns an error message, or null when the form is valid. */
  const validate = (): string | null => {
    if (!clientId.trim()) return "Toast: Client ID is required";
    if (!secretName.trim()) return "Toast: a secret name for the client secret is required";
    if (parseGuids(guids).length === 0) return "Toast: at least one restaurant GUID is required";
    return null;
  };

  const buildScaffoldConfig = (): ToastScaffoldConfig => ({
    clientId: clientId.trim(),
    clientSecretVar: secretName.trim(),
    restaurantGuids: parseGuids(guids),
    baseUrl: baseUrl.trim() || undefined
  });

  /** Stores the client secret in the secret manager. A blank value means
   *  "reuse an existing secret with this name" — skip create. */
  const persistSecret = async (pipelineName: string) => {
    if (!clientSecret.trim()) return;
    await createSecret.mutateAsync({
      name: secretName.trim(),
      value: clientSecret,
      description: `Toast OAuth2 client secret for pipeline ${pipelineName}`
    });
  };

  return {
    clientId,
    setClientId,
    clientSecret,
    setClientSecret,
    secretName,
    setSecretName,
    guids,
    setGuids,
    baseUrl,
    setBaseUrl,
    reset,
    validate,
    buildScaffoldConfig,
    persistSecret
  };
}
