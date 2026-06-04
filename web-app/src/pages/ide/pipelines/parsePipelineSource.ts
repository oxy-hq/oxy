import YAML from "yaml";

export interface QuickBooksPipelineSource {
  kind: "quickbooks";
  clientId: string;
  clientSecretVar: string;
  refreshTokenVar: string;
}

/**
 * Parse a `.airway.yml` and return its QuickBooks source config, or `null`
 * if the pipeline isn't a QuickBooks source (or the required fields are
 * missing / the YAML is unparsable). Used to drive the Reconnect button.
 */
export function parseQuickBooksSource(yamlText: string): QuickBooksPipelineSource | null {
  let doc: unknown;
  try {
    doc = YAML.parse(yamlText);
  } catch {
    return null;
  }
  const source = (doc as { source?: { kind?: unknown; config?: Record<string, unknown> } })?.source;
  if (source?.kind !== "quickbooks") return null;
  const config = source.config ?? {};
  const clientId = String(config.client_id ?? "");
  const clientSecretVar = String(config.client_secret_var ?? "");
  const refreshTokenVar = String(config.refresh_token_var ?? "");
  if (!clientId || !clientSecretVar || !refreshTokenVar) return null;
  return { kind: "quickbooks", clientId, clientSecretVar, refreshTokenVar };
}
