import { Database, GitFork } from "lucide-react";
import { useMemo } from "react";
import { useLocation, useNavigate, useParams } from "react-router-dom";
import { useAuth } from "@/contexts/AuthContext";
import useAgents from "@/hooks/api/agents/useAgents";
import useDatabases from "@/hooks/api/databases/useDatabases";
import useGithubSetup from "@/hooks/api/onboarding/useGithubSetup";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import {
  hasPendingOnboardingForStorageKey,
  isOnboardingDismissedForStorageKey
} from "@/libs/utils/onboardingStorage";
import ROUTES from "@/libs/utils/routes";
import { getAgentNameFromPath } from "@/libs/utils/string";
import useCurrentOrg from "@/stores/useCurrentOrg";
import useSettingsDialog from "@/stores/useSettingsDialog";

export interface SetupGap {
  icon: typeof Database;
  label: string;
  action: () => void;
  cta: string;
}

export type WorkspaceReadiness =
  | { status: "loading" }
  | { status: "redirect-onboarding"; to: string }
  | { status: "ready"; gaps: SetupGap[]; shouldDisableChat: boolean };

/**
 * The old chat-home's gating logic, extracted: workspace match, setup
 * gaps (per-agent LLM key resolution, warehouse creds, no-db/no-agent),
 * and the pending-wizard redirect. Behavior is copied verbatim from
 * pages/home — see git history of pages/home/index.tsx for the original
 * inline comments and rationale.
 */
export default function useWorkspaceReadiness(): WorkspaceReadiness {
  const { isLocalMode } = useAuth();
  const { project } = useCurrentProjectBranch();
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";
  const navigate = useNavigate();
  const location = useLocation();
  const openSettings = useSettingsDialog((s) => s.open);
  const { wsId: urlWsId } = useParams<{ wsId: string }>();
  const locationState = location.state as { agentPath?: string } | null;

  // The Zustand store lags one render behind workspace switches; gate every
  // query so we don't decide a redirect using the previous workspace's data.
  // In local mode there is no :wsId URL segment, so skip the URL check.
  const wsMatch = !!project?.id && (isLocalMode || project.id === urlWsId);

  // In cloud mode `github_setup` checks only DB secrets, so a key set via an
  // env var on the server is still reported missing — intentional, to prompt
  // operators to configure it through the UI. In local mode the endpoint also
  // checks env vars, so keys in .env are treated as present.
  const {
    data: githubSetup,
    isPending: setupPending,
    isError: setupError
  } = useGithubSetup(wsMatch);
  const { data: agents = [], isPending: agentsPending, isError: agentsError } = useAgents();
  const {
    data: databases = [],
    isPending: databasesPending,
    isError: databasesError
  } = useDatabases(wsMatch);

  // Mirror AgentsDropdown's default-agent logic. We rely on `agent.model`
  // already being on each listing item — fetching the agent's full config
  // here would block the home render on a serial round-trip.
  const defaultAgent = useMemo(() => {
    const publicAgents = agents.filter((a) => a.public);
    if (publicAgents.length === 0) return null;
    if (locationState?.agentPath) {
      const preferred = publicAgents.find((a) => a.path === locationState.agentPath);
      if (preferred) return preferred;
    }
    return [...publicAgents].sort((a, b) =>
      (a.name ?? getAgentNameFromPath(a.path)).localeCompare(b.name ?? getAgentNameFromPath(b.path))
    )[0];
  }, [agents, locationState?.agentPath]);

  if (!wsMatch || setupPending || agentsPending || databasesPending) {
    return { status: "loading" };
  }

  const routes = ROUTES.ORG(orgSlug).WORKSPACE(project.id);

  // Fall through on API errors so a broken endpoint doesn't trap the user.
  const anyApiError = setupError || agentsError || databasesError;
  const missingLlmKeys = githubSetup?.missing_llm_key_vars ?? [];
  const missingLlmKeyVars = new Set(missingLlmKeys.map((k) => k.var_name));
  // DuckDB is file-backed; its `password_var` is a config artifact, not a prompt.
  const warehousesNeedingCreds = (githubSetup?.warehouses ?? []).filter(
    (w) => w.dialect.toLowerCase() !== "duckdb" && w.missing_vars.length > 0
  );

  // Tie the LLM gap to the agent the chat actually uses, not to any-key-missing
  // — otherwise a saved Anthropic key wouldn't suppress the warning when the
  // active agent is on Anthropic but other unused OpenAI models still lack a
  // key. Two resolution paths so older backends still work:
  //   1. `models[]` (new) — full model -> key_var map.
  //   2. `missing_llm_key_vars[].sample_model_name` (existing) — partial,
  //      only resolves when the agent's model is the dedupe-winning sample.
  const modelKeyVarMap = new Map((githubSetup?.models ?? []).map((m) => [m.name, m.key_var]));
  const defaultAgentModel = defaultAgent?.model;
  const matchedBySample = defaultAgentModel
    ? missingLlmKeys.find((k) => k.sample_model_name === defaultAgentModel)?.var_name
    : undefined;
  const resolvedKeyVar: string | null | undefined =
    defaultAgentModel !== undefined && modelKeyVarMap.has(defaultAgentModel)
      ? (modelKeyVarMap.get(defaultAgentModel) ?? null)
      : matchedBySample !== undefined
        ? matchedBySample
        : undefined;
  const llmKeyMissingForAgent =
    resolvedKeyVar === undefined
      ? missingLlmKeys.length > 0
      : resolvedKeyVar !== null && missingLlmKeyVars.has(resolvedKeyVar);

  const hasDatabases = databases.length > 0;
  const hasPublicAgents = agents.filter((a) => a.public).length > 0;
  const hasWarehouseCredentials = warehousesNeedingCreds.length === 0;

  // We don't redirect on `!hasDatabases` / `!hasPublicAgents` because those
  // gaps aren't fixable in the wizard — they need config.yml edits, so the
  // toast below points the user at the IDE / Settings instead.
  const projectStorageKey = project.storage_key ?? project.id;
  const hasPendingWizardState = hasPendingOnboardingForStorageKey(projectStorageKey);
  const onboardingDismissed = isOnboardingDismissedForStorageKey(projectStorageKey);
  const hasMissingCredentials = !anyApiError && (llmKeyMissingForAgent || !hasWarehouseCredentials);
  // Absolute path: `home` and `onboarding` are siblings in WorkspaceLayout,
  // so relative `to='onboarding'` resolves to `/home/onboarding` (404).
  //
  // Don't force the wizard when (a) an endpoint is down — we can't trust the
  // setup state, e.g. github-setup 502s while the IDE is unreachable, and the
  // wizard would just error too; or (b) the user explicitly deferred setup via
  // "Skip for now". In both cases the gaps still surface as rows below.
  if (!anyApiError && !onboardingDismissed && (hasPendingWizardState || hasMissingCredentials)) {
    return { status: "redirect-onboarding", to: routes.ONBOARDING };
  }

  const isSetupComplete = hasDatabases && hasPublicAgents;
  // On API error we don't render any gap rows (we can't trust the data), so
  // the user would see a locked chat with no actionable steps. Let them try
  // chatting instead.
  const shouldDisableChat = !anyApiError && !isSetupComplete;

  // Credential gaps already triggered the redirect above; these are the
  // wizard-unfixable gaps (no databases / no agents).
  const gaps: SetupGap[] = [];
  if (!anyApiError) {
    if (!hasDatabases) {
      gaps.push({
        icon: Database,
        label: "No database connection",
        action: () => openSettings("workspace.databases"),
        cta: "Configure"
      });
    }
    if (!hasPublicAgents) {
      gaps.push({
        icon: GitFork,
        label: "No agents configured",
        action: () => navigate(routes.IDE.ROOT),
        cta: "Open IDE"
      });
    }
  }

  return { status: "ready", gaps, shouldDisableChat };
}
