/**
 * Per-workspace localStorage for the agentic onboarding wizard. Lives at the
 * utility layer so foundational consumers (AuthContext, Home) don't need to
 * reach into the wizard's internals to read/clear it.
 *
 * Keys are `oxy_onboarding_state:{storage_key}`, where `storage_key` comes
 * from the server (`WorkspaceDetailsResponse`): workspace UUID in cloud,
 * `local:{path-hash}` in local — see `compute_workspace_storage_key` in
 * the Rust side for the path-derivation contract.
 */

import type {
  OnboardingMode,
  OnboardingState,
  OnboardingStep
} from "@/components/workspaces/components/CreateWorkspaceDialog/components/types";

const STORAGE_KEY_PREFIX = "oxy_onboarding_state:";
export const LEGACY_GLOBAL_KEY = "oxy_onboarding_state";
/** Pre-`storage_key` local-mode entries were keyed under the nil UUID. */
const LEGACY_LOCAL_NIL_UUID_KEY = `${STORAGE_KEY_PREFIX}00000000-0000-0000-0000-000000000000`;

export function storageKey(storageKeyId: string): string {
  return `${STORAGE_KEY_PREFIX}${storageKeyId}`;
}

/** Fields safe to persist (no credentials). */
export type PersistableState = Omit<OnboardingState, "llmApiKey" | "warehouseCredentials">;

export const VALID_STEPS: ReadonlySet<OnboardingStep> = new Set<OnboardingStep>([
  "welcome",
  "llm_provider",
  "llm_model",
  "llm_key",
  "warehouse_type",
  "warehouse_credentials",
  "connection_test",
  "schema_discovery",
  "table_selection",
  "building",
  "github_loading",
  "github_llm_keys",
  "github_warehouse_creds",
  "github_connection_test",
  "complete"
]);

export function initOnboardingStateForStorageKey(
  storageKeyId: string,
  mode: OnboardingMode = "new"
): void {
  if (!storageKeyId) return;
  try {
    // Demo workspaces reuse the github flow's "inspect existing config.yml,
    // collect missing secrets" shape — the only difference is that the demo
    // is always DuckDB so the warehouse step is filtered out at fetch time.
    const reusesGithubFlow = mode === "github" || mode === "demo";
    const step: OnboardingStep = reusesGithubFlow ? "github_loading" : "welcome";
    const seeded: PersistableState = {
      step,
      storageKey: storageKeyId,
      mode,
      connectionStatus: "idle",
      discoveredSchemas: [],
      selectedTables: []
    };
    localStorage.setItem(storageKey(storageKeyId), JSON.stringify(seeded));
  } catch {
    // localStorage may be unavailable
  }
}

function getPersistedStepForStorageKey(storageKeyId: string): OnboardingStep | undefined {
  if (!storageKeyId) return undefined;
  try {
    const raw = localStorage.getItem(storageKey(storageKeyId));
    if (!raw) return undefined;
    const parsed = JSON.parse(raw) as PersistableState;
    if (parsed?.storageKey !== storageKeyId) return undefined;
    if (typeof parsed.step !== "string" || !VALID_STEPS.has(parsed.step)) return undefined;
    return parsed.step;
  } catch {
    return undefined;
  }
}

export function hasPendingOnboardingForStorageKey(storageKeyId: string): boolean {
  const step = getPersistedStepForStorageKey(storageKeyId);
  return step !== undefined && step !== "complete";
}

export function clearOnboardingStateForStorageKey(storageKeyId: string): void {
  if (!storageKeyId) return;
  try {
    localStorage.removeItem(storageKey(storageKeyId));
  } catch {
    // ignore
  }
}

export function clearAllOnboardingState(): void {
  try {
    localStorage.removeItem(LEGACY_GLOBAL_KEY);
    const toRemove: string[] = [];
    for (let i = 0; i < localStorage.length; i++) {
      const k = localStorage.key(i);
      if (k?.startsWith(STORAGE_KEY_PREFIX)) toRemove.push(k);
    }
    for (const k of toRemove) localStorage.removeItem(k);
  } catch {
    // ignore
  }
}

/** One-shot cleanup for upgraders carrying a pre-`storage_key` entry. */
export function clearLegacyLocalOnboardingState(): void {
  try {
    localStorage.removeItem(LEGACY_LOCAL_NIL_UUID_KEY);
  } catch {
    // ignore
  }
}
