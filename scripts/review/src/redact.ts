// Secrets and the reviewer. The model reads untrusted text (the diff, PR files), so a
// prompt injection could ask it to echo a secret into a finding, and findings are
// posted as PR comments, which log masking does not cover. Two defences: the agent's
// environment carries no secret it does not need, and every posted text is scrubbed of
// the secrets the job does hold.

type Env = Record<string, string | undefined>;

const SECRET_NAME = /(TOKEN|SECRET|PASSWORD|PRIVATE|CREDENTIAL|_KEY$|^KEY$)/i;
// The agent process itself needs these to call the model.
const AGENT_NEEDS = new Set(["ANTHROPIC_API_KEY", "CLAUDE_CODE_OAUTH_TOKEN"]);

/** The environment handed to the agent: everything except secrets it has no use for. */
export function agentEnv(env: Env): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [k, v] of Object.entries(env))
    if (v !== undefined && (AGENT_NEEDS.has(k) || !SECRET_NAME.test(k))) out[k] = v;
  return out;
}

/** Secret values present in an environment, longest first, too long to be ordinary words. */
export function secretValues(env: Env): string[] {
  return Object.entries(env)
    .filter(
      (e): e is [string, string] =>
        SECRET_NAME.test(e[0]) && typeof e[1] === "string" && e[1].length >= 12
    )
    .map(([, v]) => v)
    .sort((a, b) => b.length - a.length);
}

export function redact(text: string | null | undefined, secrets: readonly string[]): string {
  let out = text ?? "";
  for (const s of secrets) out = out.split(s).join("[redacted]");
  return out;
}
