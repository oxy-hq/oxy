/**
 * Per-workspace localStorage for the agentic onboarding wizard. Lives at the
 * utility layer so foundational consumers (AuthContext, Home) don't need to
 * reach into the wizard's internals to read/clear it.
 *
 * Keys are namespaced as `oxy_onboarding_state:{workspaceId}` so concurrent
 * onboardings on different workspaces don't stomp on each other and a stored
 * blob can never hydrate the wrong workspace's wizard.
 */

import type {
  OnboardingMode,
  OnboardingState,
  OnboardingStep
} from "@/components/workspaces/components/CreateWorkspaceDialog/components/types";

export const STORAGE_KEY_PREFIX = "oxy_onboarding_state:";
export const LEGACY_GLOBAL_KEY = "oxy_onboarding_state";

export function storageKey(workspaceId: string): string {
  return `${STORAGE_KEY_PREFIX}${workspaceId}`;
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

export function initOnboardingStateForWorkspace(
  workspaceId: string,
  mode: OnboardingMode = "new"
): void {
  if (!workspaceId) return;
  try {
    // Demo workspaces reuse the github flow's "inspect existing config.yml,
    // collect missing secrets" shape — the only difference is that the demo
    // is always DuckDB so the warehouse step is filtered out at fetch time.
    const reusesGithubFlow = mode === "github" || mode === "demo";
    const step: OnboardingStep = reusesGithubFlow ? "github_loading" : "welcome";
    const seeded: PersistableState = {
      step,
      workspaceId,
      mode,
      connectionStatus: "idle",
      discoveredSchemas: [],
      selectedTables: []
    };
    localStorage.setItem(storageKey(workspaceId), JSON.stringify(seeded));
  } catch {
    // localStorage may be unavailable
  }
}

export function getPersistedStepForWorkspace(workspaceId: string): OnboardingStep | undefined {
  if (!workspaceId) return undefined;
  try {
    const raw = localStorage.getItem(storageKey(workspaceId));
    if (!raw) return undefined;
    const parsed = JSON.parse(raw) as PersistableState;
    if (parsed?.workspaceId !== workspaceId) return undefined;
    if (typeof parsed.step !== "string" || !VALID_STEPS.has(parsed.step)) return undefined;
    return parsed.step;
  } catch {
    return undefined;
  }
}

export function hasPendingOnboardingForWorkspace(workspaceId: string): boolean {
  const step = getPersistedStepForWorkspace(workspaceId);
  return step !== undefined && step !== "complete";
}

export function clearOnboardingStateForWorkspace(workspaceId: string): void {
  if (!workspaceId) return;
  try {
    localStorage.removeItem(storageKey(workspaceId));
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
