import { useEffect, useState } from "react";
import { useCreateSecret } from "@/hooks/api/secrets/useSecretMutations";
import useSecrets from "@/hooks/api/secrets/useSecrets";
import useSpApiMarketplaces from "@/hooks/api/spApi/useSpApiMarketplaces";
import { firstOfLastMonth } from "../../../scaffold";

export interface SpApiScaffoldConfig {
  clientId: string;
  clientSecretVar: string;
  refreshTokenVar: string;
  marketplaceId: string;
  defaultStart: string;
}

/** `YYYY-MM-DD`, which is what `<input type="date">` produces and what the
 *  connector's `parse_default_start` accepts alongside RFC 3339. */
const ISO_DATE = /^\d{4}-\d{2}-\d{2}$/;

/** Owns the Amazon SP-API credential form state, validation, secret
 *  persistence and scaffold-config shaping for the New Pipeline wizard.
 *
 * No OAuth flow, unlike QuickBooks: an SP-API refresh token is issued when the
 * app is authorized in Seller Central, so the operator already holds one and
 * only needs to name the secret it lives in.
 */
export function useSpApiCredentials() {
  const [clientId, setClientId] = useState("");
  const [clientSecret, setClientSecret] = useState("");
  const [clientSecretName, setClientSecretName] = useState("SP_API_CLIENT_SECRET");
  const [refreshToken, setRefreshToken] = useState("");
  const [refreshTokenName, setRefreshTokenName] = useState("SP_API_REFRESH_TOKEN");
  // Server-owned, so the picker cannot drift from `build_sp_api`'s refusal.
  // Empty until it loads; `validate` treats that as "not chosen yet" rather
  // than letting a blank reach the YAML.
  const marketplaces = useSpApiMarketplaces();
  const [marketplaceId, setMarketplaceId] = useState<string>("");

  // Select the first once the list arrives, so the form is valid by default
  // rather than making every operator pick before they can continue. Only
  // when nothing is chosen yet — never clobbering a real choice, including
  // after a refetch.
  useEffect(() => {
    const first = marketplaces.data?.[0]?.id;
    if (first && !marketplaceId) setMarketplaceId(first);
  }, [marketplaces.data, marketplaceId]);
  const [defaultStart, setDefaultStart] = useState(firstOfLastMonth());
  const createSecret = useCreateSecret();
  // Needed to tell "reuse the existing secret" from "reference one that was
  // never created" — a blank value means the former, and without this the two
  // are indistinguishable at the point the operator can still fix it.
  const secrets = useSecrets();

  const reset = () => {
    setClientId("");
    setClientSecret("");
    setClientSecretName("SP_API_CLIENT_SECRET");
    setRefreshToken("");
    setRefreshTokenName("SP_API_REFRESH_TOKEN");
    setMarketplaceId("");
    setDefaultStart(firstOfLastMonth());
  };

  /** Returns an error message, or null when the form is valid.
   *
   * These mirror `build_sp_api`'s own refusals. Catching them here turns a
   * failed pipeline run into a form error the operator can act on.
   */
  const validate = (): string | null => {
    if (!clientId.trim()) return "Amazon SP-API: LWA Client ID is required";
    if (!clientSecretName.trim())
      return "Amazon SP-API: a secret name for the client secret is required";
    if (!refreshTokenName.trim())
      return "Amazon SP-API: a secret name for the refresh token is required";
    // Checked against the SERVER's list, so this cannot disagree with the
    // refusal `build_sp_api` would raise at run time.
    if (!marketplaceId) return "Amazon SP-API: a marketplace is required";
    if (marketplaces.data && !marketplaces.data.some((m) => m.id === marketplaceId))
      return "Amazon SP-API: pick a North America marketplace — the connector only reaches that endpoint";
    // Required, not defaulted: this is the whole backfill policy for a
    // forward-only connector, and both a too-recent and a too-old value fail
    // silently (missing history, or a first run that exhausts the report
    // budget). The backend refuses without it rather than guessing.
    if (!defaultStart.trim()) return "Amazon SP-API: a start date is required";
    if (!ISO_DATE.test(defaultStart.trim())) return "Amazon SP-API: start date must be YYYY-MM-DD";

    // A blank value means "reuse the secret already stored under this name".
    // Nothing checked that one existed, so with every other field defaulted,
    // typing only the client id wrote a pipeline whose `*_var` names point at
    // secrets nobody created. The first symptom was a run failing with
    // "`client_secret` is empty or unset", which reads as "the secret is
    // wrong" rather than "you never made one".
    //
    // Checked against EACH OTHER before the server list, because the server
    // cannot see this one: two brand-new secrets sharing a name both pass the
    // "does not exist yet" test, then `persistSecret` runs two sequential
    // creates on that name and the second 409s — stranding the first, which
    // now holds the client secret under a name the operator will next read as
    // a refresh token. Same failure the server-side check closes, reached
    // through the form instead.
    if (clientSecretName.trim() === refreshTokenName.trim()) {
      return "Amazon SP-API: the client secret and the refresh token need different secret names";
    }

    // Only enforced once the list has loaded — a failed secrets query should
    // not block the wizard, it just costs this particular check.
    const existing = secrets.data?.secrets;
    if (existing) {
      const has = (name: string) => existing.some((s) => s.name === name);
      for (const [value, name, field] of [
        [clientSecret, clientSecretName, "client secret"],
        [refreshToken, refreshTokenName, "refresh token"]
      ] as const) {
        const trimmedName = name.trim();
        if (!value.trim() && !has(trimmedName)) {
          return `Amazon SP-API: no secret named ${trimmedName} exists — paste the ${field} so it can be created, or point at an existing secret`;
        }
        // Creating over an existing name 409s. Worse, the two creates run in
        // sequence, so a collision on the second strands the first and every
        // retry hits the same 409.
        if (value.trim() && has(trimmedName)) {
          return `Amazon SP-API: a secret named ${trimmedName} already exists — clear the ${field} to reuse it, or choose another name`;
        }
      }
    }
    return null;
  };

  const buildScaffoldConfig = (): SpApiScaffoldConfig => ({
    clientId: clientId.trim(),
    clientSecretVar: clientSecretName.trim(),
    refreshTokenVar: refreshTokenName.trim(),
    marketplaceId,
    defaultStart: defaultStart.trim()
  });

  /** Stores both credentials in the secret manager. A blank value means
   *  "reuse an existing secret with this name" — skip create, same as Toast. */
  const persistSecret = async (pipelineName: string) => {
    if (clientSecret.trim()) {
      await createSecret.mutateAsync({
        name: clientSecretName.trim(),
        value: clientSecret,
        description: `Amazon LWA client secret for pipeline ${pipelineName}`
      });
    }
    if (refreshToken.trim()) {
      await createSecret.mutateAsync({
        name: refreshTokenName.trim(),
        value: refreshToken,
        description: `Amazon SP-API refresh token for pipeline ${pipelineName}`
      });
    }
  };

  return {
    clientId,
    setClientId,
    clientSecret,
    setClientSecret,
    clientSecretName,
    setClientSecretName,
    refreshToken,
    setRefreshToken,
    refreshTokenName,
    setRefreshTokenName,
    marketplaceId,
    setMarketplaceId,
    marketplaces,
    defaultStart,
    setDefaultStart,
    reset,
    validate,
    buildScaffoldConfig,
    persistSecret
  };
}
