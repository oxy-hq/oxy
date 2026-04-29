import { AlertCircle, ArrowRight, Database, GitFork, Key, Lock, X } from "lucide-react";
import { useMemo, useState } from "react";
import { Link, Navigate, useLocation, useParams } from "react-router-dom";
import ChatPanel from "@/components/Chat/ChatPanel";
import PageHeader from "@/components/PageHeader";
import useSidebar from "@/components/ui/shadcn/sidebar-context";
import { Spinner } from "@/components/ui/shadcn/spinner";
import useAgents from "@/hooks/api/agents/useAgents";
import useDatabases from "@/hooks/api/databases/useDatabases";
import useGithubSetup from "@/hooks/api/onboarding/useGithubSetup";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { cn } from "@/libs/shadcn/utils";
import { hasPendingOnboardingForWorkspace } from "@/libs/utils/onboardingStorage";
import ROUTES from "@/libs/utils/routes";
import { getAgentNameFromPath } from "@/libs/utils/string";
import useCurrentOrg from "@/stores/useCurrentOrg";

const getGreeting = () => {
  const hour = new Date().getHours();
  if (hour < 12) return "Good Morning";
  if (hour < 18) return "Good Afternoon";
  return "Good Evening";
};

interface SetupGap {
  icon: typeof Database;
  label: string;
  to: string;
  cta: string;
}

const ProjectSetupToast = ({ gaps }: { gaps: SetupGap[] }) => {
  const [dismissed, setDismissed] = useState(false);
  if (dismissed || gaps.length === 0) return null;

  return (
    <div className='fade-in slide-in-from-top-2 fixed top-4 right-4 z-50 w-96 animate-in duration-300'>
      <div className='rounded-lg border border-amber-500/30 bg-background shadow-black/10 shadow-lg'>
        <div className='flex items-center justify-between border-amber-500/20 border-b px-4 py-3'>
          <div className='flex items-center gap-2 text-amber-600 dark:text-amber-400'>
            <AlertCircle className='h-3.5 w-3.5 shrink-0' />
            <span className='font-medium text-xs'>Project setup incomplete</span>
          </div>
          <button
            type='button'
            onClick={() => setDismissed(true)}
            className='rounded p-0.5 text-muted-foreground/40 transition-colors hover:bg-muted hover:text-muted-foreground'
            aria-label='Dismiss'
          >
            <X className='h-3.5 w-3.5' />
          </button>
        </div>
        <div className='flex flex-col gap-1 p-2'>
          {gaps.map((gap) => (
            <div
              key={gap.label}
              className='flex items-center justify-between gap-3 rounded-md px-2 py-2'
            >
              <div className='flex min-w-0 items-center gap-2 text-muted-foreground text-xs'>
                <gap.icon className='h-3 w-3 shrink-0' />
                <span className='truncate'>{gap.label}</span>
              </div>
              <Link
                to={gap.to}
                className='flex shrink-0 items-center gap-1 whitespace-nowrap font-medium text-primary text-xs hover:underline'
              >
                {gap.cta}
                <ArrowRight className='h-3 w-3' />
              </Link>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
};

const Home = () => {
  const { open } = useSidebar();
  const { project } = useCurrentProjectBranch();
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";
  const location = useLocation();
  const { wsId: urlWsId } = useParams<{ wsId: string }>();
  const locationState = location.state as {
    prefillQuestion?: string;
    agentPath?: string;
    autoSubmit?: boolean;
  } | null;

  // The Zustand store lags one render behind workspace switches; gate every
  // query so we don't decide a redirect using the previous workspace's data.
  const wsMatch = !!project?.id && project.id === urlWsId;

  // `github_setup` checks workspace secrets (env-var fallback doesn't count),
  // so it correctly reports a missing key even when the operator has set the
  // var process-side. The readiness endpoint can't distinguish those.
  const {
    data: githubSetup,
    isPending: setupPending,
    isError: setupError
  } = useGithubSetup(wsMatch);
  const { data: agents = [], isPending: agentsPending, isError: agentsError } = useAgents(wsMatch);
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

  const loadingFallback = (
    <div className='flex h-full items-center justify-center'>
      <Spinner className='size-6' />
    </div>
  );

  if (!wsMatch) {
    return loadingFallback;
  }

  const routes = ROUTES.ORG(orgSlug).WORKSPACE(project.id);

  // Absolute path: `home` and `onboarding` are siblings in WorkspaceLayout,
  // so relative `to='onboarding'` resolves to `/home/onboarding` (404).
  if (hasPendingOnboardingForWorkspace(project.id)) {
    return <Navigate to={routes.ONBOARDING} replace />;
  }

  if (setupPending || agentsPending || databasesPending) {
    return loadingFallback;
  }

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
  const llmGapLabel =
    resolvedKeyVar !== undefined && resolvedKeyVar !== null && defaultAgent
      ? `${resolvedKeyVar} not set for ${defaultAgent.name ?? getAgentNameFromPath(defaultAgent.path)}`
      : "No LLM API key";

  const hasDatabases = databases.length > 0;
  const hasPublicAgents = agents.filter((a) => a.public).length > 0;
  const hasWarehouseCredentials = warehousesNeedingCreds.length === 0;
  const isSetupComplete =
    !llmKeyMissingForAgent && hasDatabases && hasPublicAgents && hasWarehouseCredentials;
  // On API error we don't render any gap rows (we can't trust the data), so
  // the user would see a locked chat with no actionable steps. Let them try
  // chatting instead.
  const shouldDisableChat = !anyApiError && !isSetupComplete;

  // Surface gaps as a toast rather than redirecting — the wizard is one-shot
  // and ends in a dead-end "complete" state, and the user just came from
  // there. Links go straight to the Secrets page.
  const gaps: SetupGap[] = [];
  if (!anyApiError) {
    if (llmKeyMissingForAgent) {
      gaps.push({
        icon: Key,
        label: llmGapLabel,
        to: routes.IDE.SETTINGS.SECRETS,
        cta: "Add key"
      });
    }
    if (!hasWarehouseCredentials) {
      const names = warehousesNeedingCreds.map((w) => w.name).join(", ");
      gaps.push({
        icon: Lock,
        label:
          warehousesNeedingCreds.length === 1
            ? `Missing credentials for ${names}`
            : "Missing warehouse credentials",
        to: routes.IDE.SETTINGS.SECRETS,
        cta: "Add credentials"
      });
    }
    if (!hasDatabases) {
      gaps.push({
        icon: Database,
        label: "No database connection",
        to: routes.IDE.SETTINGS.DATABASES,
        cta: "Configure"
      });
    }
    if (!hasPublicAgents) {
      gaps.push({
        icon: GitFork,
        label: "No agents configured",
        to: routes.IDE.ROOT,
        cta: "Open IDE"
      });
    }
  }

  const greeting = getGreeting();

  return (
    <div className='flex h-full flex-col'>
      {!open && <PageHeader />}
      <ProjectSetupToast gaps={gaps} />
      <div className='flex h-full flex-col items-center justify-center gap-10 px-4'>
        <p className='text-center text-3xl'>{greeting}! How can I assist you?</p>

        <div className='flex w-full max-w-4xl flex-col items-center gap-3 pb-40'>
          {shouldDisableChat && (
            <p className='text-center text-muted-foreground/50 text-xs'>
              Complete the setup steps above to start chatting.
            </p>
          )}
          <div
            className={cn(
              "w-full",
              shouldDisableChat && "pointer-events-none select-none opacity-40"
            )}
          >
            <ChatPanel
              initialMessage={locationState?.prefillQuestion}
              initialAgentPath={locationState?.agentPath}
              autoSubmit={locationState?.autoSubmit}
            />
          </div>
        </div>
      </div>
    </div>
  );
};

export default Home;
