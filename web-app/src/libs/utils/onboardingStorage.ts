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
/**
 * Marks that the user chose to leave setup incomplete for a workspace (e.g.
 * clicked "Skip for now"). Separate from the wizard-state key so dismissing
 * the redirect doesn't disturb any in-progress step the user may resume.
 */
const DISMISS_KEY_PREFIX = "oxy_onboarding_dismissed:";
export const LEGACY_GLOBAL_KEY = "oxy_onboarding_state";
/** Pre-`storage_key` local-mode entries were keyed under the nil UUID. */
const LEGACY_LOCAL_NIL_UUID_KEY = `${STORAGE_KEY_PREFIX}00000000-0000-0000-0000-000000000000`;

export function storageKey(storageKeyId: string): string {
  return `${STORAGE_KEY_PREFIX}${storageKeyId}`;
}

function dismissKey(storageKeyId: string): string {
  return `${DISMISS_KEY_PREFIX}${storageKeyId}`;
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
    // Keep the two in sync: clearing wizard state (e.g. "Start over") should
    // re-enable the setup redirect rather than leave a stale dismissal.
    localStorage.removeItem(dismissKey(storageKeyId));
  } catch {
    // ignore
  }
}

/**
 * Record that the user deferred setup for this workspace. Home reads this to
 * stop force-redirecting into the wizard — the missing-credential check alone
 * keeps re-triggering the redirect on every visit otherwise. The gaps still
 * surface as dismissible rows on Home, and the wizard stays reachable.
 */
export function markOnboardingDismissedForStorageKey(storageKeyId: string): void {
  if (!storageKeyId) return;
  try {
    localStorage.setItem(dismissKey(storageKeyId), "1");
  } catch {
    // localStorage may be unavailable
  }
}

/**
 * Drop the deferral without touching any in-progress wizard state.
 *
 * `CompletionCard` sets the dismissal on EVERY successful completion, not just
 * on "Skip for now", and nothing else clears it — so the workspace creator's
 * browser carries it forever. Home reads it to decide whether to run the
 * IdeOnly setup probe, so a flag that never clears means that browser keeps
 * hitting an ide-pinned route on every visit with no setup left to finish.
 * Home clears it once the workspace is provably ready.
 *
 * Deliberately NOT `clearOnboardingStateForStorageKey`, which also drops the
 * wizard state and would break a resume.
 */
export function clearOnboardingDismissedForStorageKey(storageKeyId: string): void {
  if (!storageKeyId) return;
  try {
    localStorage.removeItem(dismissKey(storageKeyId));
  } catch {
    // ignore
  }
}

export function isOnboardingDismissedForStorageKey(storageKeyId: string): boolean {
  if (!storageKeyId) return false;
  try {
    return localStorage.getItem(dismissKey(storageKeyId)) === "1";
  } catch {
    return false;
  }
}

export function clearAllOnboardingState(): void {
  try {
    localStorage.removeItem(LEGACY_GLOBAL_KEY);
    const toRemove: string[] = [];
    for (let i = 0; i < localStorage.length; i++) {
      const k = localStorage.key(i);
      if (k?.startsWith(STORAGE_KEY_PREFIX) || k?.startsWith(DISMISS_KEY_PREFIX)) toRemove.push(k);
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
