import { Database, GitFork, KeyRound } from "lucide-react";
import { useEffect, useMemo } from "react";
import { useLocation, useNavigate, useParams } from "react-router-dom";
import { useAuth } from "@/contexts/AuthContext";
import useAgents from "@/hooks/api/agents/useAgents";
import useDatabases from "@/hooks/api/databases/useDatabases";
import useGithubSetup from "@/hooks/api/onboarding/useGithubSetup";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import {
  clearOnboardingDismissedForStorageKey,
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
 * and the pending-wizard redirect. Originally copied verbatim from
 * pages/home — see git history of pages/home/index.tsx for the original
 * inline comments and rationale.
 *
 * The redirect since narrowed to "a wizard is actually pending on a workspace
 * that isn't usable yet". Missing credentials alone report as gap rows; see
 * the comment at the redirect for why the probe can't be trusted to mean the
 * user never onboarded — and the comment at `needsSetupProbe` for why Home
 * doesn't even make that call on the common path.
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

  // Falling back to `project.id` keeps fabricated/partial Workspace objects
  // (legacy callers; tests) working until they fill in storage_key.
  const projectStorageKey = project?.storage_key ?? project?.id ?? "";
  const hasPendingWizardState = hasPendingOnboardingForStorageKey(projectStorageKey);
  const onboardingDismissed = isOnboardingDismissedForStorageKey(projectStorageKey);

  // `onboarding/github-setup` reads config.yml off the working copy, so the
  // manifest pins the whole `/{workspace_id}/onboarding/*` subtree to IdeOnly
  // (`role_manifest.rs`) — and unlike `/details` and `/status` it is NOT in
  // `degrades_when_ide_unreachable`, so a serve replica answers 502 with
  // `x-oxy-required-role: ide` while the Factory pod restarts. The Axios
  // interceptor turns that into the app-wide "Oxygen Factory is temporarily
  // unavailable" banner. Firing it on every Home load meant every rollout put
  // that banner in front of every user, none of whom had asked for anything
  // the Factory owns.
  //
  // So Home only probes when THIS browser is tracking a setup for THIS
  // workspace (mid-wizard, or deferred via "Skip for now"), plus legacy local
  // mode where there's no fleet and the probe is the only entry into setup.
  // Everyone else gets Home from the FleetOk `/agents` + `/databases` reads.
  const needsSetupProbe = wsMatch && (hasPendingWizardState || onboardingDismissed || isLocalMode);

  // In cloud mode `github_setup` checks only DB secrets, so a key set via an
  // env var on the server is still reported missing — intentional, to prompt
  // operators to configure it through the UI. In local mode the endpoint also
  // checks env vars, so keys in .env are treated as present.
  const {
    data: githubSetup,
    isPending: setupPending,
    isError: setupError
  } = useGithubSetup(needsSetupProbe);
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

  // `enabled: false` suppresses FETCHING, not cache reads — and AgenticSetup
  // populates this very query key, so a Home visit after `/onboarding` would
  // otherwise render gap rows off a cached payload that can never refresh (a
  // key the user just saved would still read as missing). Gate the DATA on the
  // probe, not just the request, so "we didn't ask" really does mean "we have
  // no verdict".
  const setup = needsSetupProbe ? githubSetup : undefined;

  // Fall through on API errors so a broken endpoint doesn't trap the user.
  const anyApiError = setupError || agentsError || databasesError;
  const missingLlmKeys = setup?.missing_llm_key_vars ?? [];
  const missingLlmKeyVars = new Set(missingLlmKeys.map((k) => k.var_name));
  // DuckDB is file-backed; its `password_var` is a config artifact, not a prompt.
  const warehousesNeedingCreds = (setup?.warehouses ?? []).filter(
    (w) => w.dialect.toLowerCase() !== "duckdb" && w.missing_vars.length > 0
  );

  // Tie the LLM gap to the agent the chat actually uses, not to any-key-missing
  // — otherwise a saved Anthropic key wouldn't suppress the warning when the
  // active agent is on Anthropic but other unused OpenAI models still lack a
  // key. Two resolution paths so older backends still work:
  //   1. `models[]` (new) — full model -> key_var map.
  //   2. `missing_llm_key_vars[].sample_model_name` (existing) — partial,
  //      only resolves when the agent's model is the dedupe-winning sample.
  const modelKeyVarMap = new Map((setup?.models ?? []).map((m) => [m.name, m.key_var]));
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
  //
  // With the probe off there is no credential verdict at all — both halves read
  // false off the undefined `setup`. That's the intended reading: no evidence of
  // a gap, rather than a gap assumed from a call we never made. The cost is
  // stated plainly in `shouldDisableChat` below.
  const hasMissingCredentials = !anyApiError && (llmKeyMissingForAgent || !hasWarehouseCredentials);

  // A missing credential is NOT on its own a reason to hijack Home into the
  // full-page wizard. The probe reads the workspace secret store only, so a key
  // supplied by an env var reads as missing on a workspace that works fine —
  // and localStorage is per-browser, so anyone who didn't run the wizard
  // themselves (a teammate, a second device) looked "un-onboarded" forever.
  // Both cases dragged a working workspace into setup on every visit. The gaps
  // now surface as rows below instead, with the wizard one click away.
  //
  // Legacy local mode is the exception: it has no workspace-creation flow to
  // seed wizard state, so the credential probe is its only entry into setup —
  // and there the probe also reads env vars, so "missing" really is missing.
  const shouldOfferWizard = hasPendingWizardState || (isLocalMode && hasMissingCredentials);

  // …and even seeded state doesn't bounce a workspace that can already answer
  // questions — abandoned wizard state used to trap the user in a loop (Home
  // sends them to the wizard, the wizard is "in flight" so it won't send them
  // back).
  const isWorkspaceReady = hasDatabases && hasPublicAgents && !hasMissingCredentials;

  // A disabled query sits at `isPending` forever, so only wait on the setup
  // probe when we actually asked for it — otherwise Home spins indefinitely.
  const isLoading =
    !wsMatch || (needsSetupProbe && setupPending) || agentsPending || databasesPending;

  // Retire the deferral once there is demonstrably nothing left to defer.
  // `CompletionCard` sets it on every successful completion, not only on "Skip
  // for now", and nothing else ever clears it — so without this the workspace
  // CREATOR keeps the flag forever, keeps probing an IdeOnly route on every
  // Home load, and keeps catching the Factory banner on every rollout with no
  // unfinished setup at all. One clean load converges that browser to the
  // no-probe path. The wizard state is left alone so a resume still works.
  const shouldRetireDismissal =
    !isLoading && !anyApiError && onboardingDismissed && isWorkspaceReady;
  useEffect(() => {
    if (shouldRetireDismissal) clearOnboardingDismissedForStorageKey(projectStorageKey);
  }, [shouldRetireDismissal, projectStorageKey]);

  if (isLoading) {
    return { status: "loading" };
  }

  const routes = ROUTES.ORG(orgSlug).WORKSPACE(project.id);

  // Absolute path: `home` and `onboarding` are siblings in WorkspaceLayout,
  // so relative `to='onboarding'` resolves to `/home/onboarding` (404).
  //
  // Don't force the wizard when (a) an endpoint is down — we can't trust the
  // setup state, e.g. github-setup 502s while the IDE is unreachable, and the
  // wizard would just error too; or (b) the user explicitly deferred setup via
  // "Skip for now". In both cases the gaps still surface as rows below.
  if (!anyApiError && !onboardingDismissed && shouldOfferWizard && !isWorkspaceReady) {
    return { status: "redirect-onboarding", to: routes.ONBOARDING };
  }

  const isSetupComplete = hasDatabases && hasPublicAgents;
  // On API error we don't render any gap rows (we can't trust the data), so
  // the user would see a locked chat with no actionable steps. Let them try
  // chatting instead.
  //
  // Deliberately NOT tied to the credential check. On the common path the probe
  // never runs, so `llmKeyMissingForAgent` is false and no key gap is known —
  // meaning a cloud workspace with a genuinely missing key shows no row here and
  // an enabled chat, and the user's first signal is a failed send. That is the
  // accepted cost of not calling an ide-pinned route on every Home load: the
  // check's false positives (env-var keys read as missing) would otherwise lock
  // chat on workspaces that work. Surfacing the gap lazily from a send failure
  // is the follow-up; the honest fix is moving the probe onto the compile
  // boundary so it can run on any replica.
  const shouldDisableChat = !anyApiError && !isSetupComplete;

  const gaps: SetupGap[] = [];
  if (!anyApiError) {
    // Credential gaps: the wizard collects exactly these secrets, so the CTA
    // opens it rather than pushing the user into Settings to guess which
    // `*_var` the config references. Offered, not forced — that's the point.
    if (llmKeyMissingForAgent) {
      gaps.push({
        icon: KeyRound,
        label: "LLM API key not set",
        action: () => navigate(routes.ONBOARDING),
        cta: "Finish setup"
      });
    }
    if (warehousesNeedingCreds.length > 0) {
      gaps.push({
        icon: KeyRound,
        label:
          warehousesNeedingCreds.length === 1
            ? `Missing credentials for ${warehousesNeedingCreds[0].name}`
            : `Missing credentials for ${warehousesNeedingCreds.length} databases`,
        action: () => navigate(routes.ONBOARDING),
        cta: "Finish setup"
      });
    }
    // Gaps the wizard can't fix — they need config.yml edits.
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
