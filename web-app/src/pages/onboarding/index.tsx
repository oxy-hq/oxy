import {
  AlertTriangle,
  ArrowLeft,
  BookOpen,
  CheckCircle2,
  Database,
  Key,
  Loader2,
  Plus
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import GithubIcon from "@/components/ui/GithubIcon";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { useAllWorkspaces } from "@/hooks/api/workspaces/useWorkspaces";
import useAuthConfig from "@/hooks/auth/useAuthConfig";
import ROUTES from "@/libs/utils/routes";
import { OnboardingService, WorkspaceService } from "@/services/api";
import type { WorkspaceSummary } from "@/services/api/workspaces";
import { GitHubOnboardingStep } from "./GitHubOnboardingStep";
import { GitHubUrlOnboardingStep } from "./GitHubUrlOnboardingStep";

// Extract a human-readable message from an API error, preferring the response body.
// For 409 Conflict, returns a specific duplicate-name message if the body isn't readable.
// Returns null (not the generic Axios "Request failed…" message) when nothing is available.
function extractErrorMessage(err: unknown): string | null {
  if (!err || typeof err !== "object") return null;
  const response = (err as { response?: { data?: unknown; status?: number } })?.response;
  const body = response?.data;
  if (typeof body === "string" && body.length > 0) return body;
  if (response?.status === 409) {
    return "A workspace with that name already exists. Please choose a different name.";
  }
  return null;
}

// Mirror the backend sanitize_project_name logic so we can validate client-side.
function sanitizeName(raw: string): string {
  const safe = raw
    .split("")
    .map((c) => (/[a-zA-Z0-9_-]/.test(c) ? c : "-"))
    .join("")
    .replace(/^-+|-+$/g, "");
  return safe || "my-workspace";
}

function suggestUniqueName(base: string, existingNames: Set<string>): string {
  if (!existingNames.has(base)) return base;
  for (let i = 2; i <= 99; i++) {
    const candidate = `${base}-${i}`;
    if (!existingNames.has(candidate)) return candidate;
  }
  return base;
}

type Step = "pick" | "github" | "new" | "loading" | "cloning" | "checks";

// ─── Existing workspace row ────────────────────────────────────────────────────

function ExistingWorkspaceRow({ workspace }: { workspace: WorkspaceSummary }) {
  const [activating, setActivating] = useState(false);

  const handleOpen = async () => {
    setActivating(true);
    try {
      await WorkspaceService.activateWorkspace(workspace.id);
      window.location.replace("/");
    } catch {
      setActivating(false);
    }
  };

  return (
    <button
      type='button'
      onClick={handleOpen}
      disabled={activating}
      className='group flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-left transition-colors hover:bg-primary/5 disabled:opacity-60'
    >
      <div className='flex h-7 w-7 shrink-0 items-center justify-center rounded-md bg-muted font-medium text-[11px] text-muted-foreground transition-colors group-hover:bg-primary/10 group-hover:text-primary'>
        {workspace.name.charAt(0).toUpperCase()}
      </div>
      <div className='min-w-0 flex-1'>
        <p className='truncate font-medium text-[13px] text-foreground transition-colors group-hover:text-primary'>
          {workspace.name}
        </p>
        {workspace.path && (
          <p className='truncate font-mono text-[10px] text-muted-foreground/50'>
            {workspace.path}
          </p>
        )}
      </div>
      {activating ? (
        <Loader2 className='h-3 w-3 shrink-0 animate-spin text-muted-foreground' />
      ) : (
        <span className='shrink-0 text-[11px] text-muted-foreground/30 transition-colors group-hover:text-primary/50'>
          Open
        </span>
      )}
    </button>
  );
}

// ─── Create option row ─────────────────────────────────────────────────────────

function CreateOption({
  icon,
  title,
  description,
  onClick,
  recommended,
  disabled
}: {
  icon: React.ReactNode;
  title: string;
  description: string;
  onClick: () => void;
  recommended?: boolean;
  disabled?: boolean;
}) {
  return (
    <button
      type='button'
      onClick={onClick}
      disabled={disabled}
      className='group flex items-center gap-3 rounded-lg px-3 py-2.5 text-left transition-colors hover:bg-primary/5 disabled:cursor-not-allowed disabled:opacity-40'
    >
      <div className='flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-muted text-muted-foreground transition-colors group-hover:bg-primary/10 group-hover:text-primary'>
        {icon}
      </div>
      <div className='min-w-0 flex-1'>
        <div className='flex items-center gap-2'>
          <p className='font-medium text-foreground text-sm transition-colors group-hover:text-primary'>
            {title}
          </p>
          {recommended && (
            <span className='rounded-full bg-primary/10 px-1.5 py-0.5 font-medium text-[10px] text-primary'>
              Recommended
            </span>
          )}
        </div>
        <p className='text-muted-foreground text-xs leading-relaxed'>{description}</p>
      </div>
    </button>
  );
}

// ─── Page ─────────────────────────────────────────────────────────────────────

export default function OnboardingPage() {
  const [step, setStep] = useState<Step>("pick");
  const [workspaceName, setWorkspaceName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [cloningWorkspaceId, setCloningWorkspaceId] = useState<string | null>(null);
  const [workspaceType, setWorkspaceType] = useState<"demo" | "new" | "github" | null>(null);
  const [missingLlmKeys, setMissingLlmKeys] = useState<string[]>([]);
  const [requiredSecrets, setRequiredSecrets] = useState<string[]>([]);

  const { data: existingWorkspaces = [], refetch: refetchWorkspaces } = useAllWorkspaces();
  const { data: authConfig } = useAuthConfig();
  const hasExisting = existingWorkspaces.length > 0;
  // Local mode = single-workspace (oxy --local). Uses URL-based GitHub import.
  const isLocal = authConfig?.single_workspace === true;

  const nameForApi = workspaceName.trim() || undefined;

  // Build a set of sanitized existing names for conflict detection.
  const existingNameSet = useMemo(
    () => new Set(existingWorkspaces.map((w) => sanitizeName(w.name))),
    [existingWorkspaces]
  );

  const typedSanitized = workspaceName.trim() ? sanitizeName(workspaceName.trim()) : null;
  const nameConflict =
    typedSanitized !== null && existingNameSet.has(typedSanitized)
      ? suggestUniqueName(typedSanitized, existingNameSet)
      : null;

  const runChecksAndProceed = async (workspaceId: string) => {
    setCloningWorkspaceId(workspaceId);
    try {
      const [readiness, status] = await Promise.all([
        OnboardingService.getReadiness(workspaceId),
        WorkspaceService.getWorkspaceStatus(workspaceId)
      ]);
      setMissingLlmKeys(readiness.llm_keys_missing ?? []);
      setRequiredSecrets(status.required_secrets ?? []);
    } catch {
      setMissingLlmKeys([]);
      setRequiredSecrets([]);
    }
    setStep("checks");
  };

  const handleDemo = async () => {
    setStep("loading");
    setError(null);
    setWorkspaceType("demo");
    try {
      const result = await OnboardingService.setupDemo(nameForApi);
      await WorkspaceService.activateWorkspace(result.workspace_id);
      await runChecksAndProceed(result.workspace_id);
    } catch (err) {
      setError(extractErrorMessage(err) ?? "Failed to set up demo workspace");
      setStep("pick");
    }
  };

  const handleNew = async () => {
    setStep("loading");
    setError(null);
    setWorkspaceType("new");
    try {
      const result = await OnboardingService.setupNew(nameForApi);
      await WorkspaceService.activateWorkspace(result.workspace_id);
      await runChecksAndProceed(result.workspace_id);
    } catch (err) {
      setError(extractErrorMessage(err) ?? "Failed to create workspace");
      setStep("new");
    }
  };

  const handleGitHubDone = async (workspaceId: string) => {
    setWorkspaceType("github");
    try {
      await WorkspaceService.activateWorkspace(workspaceId);
    } catch {
      // best-effort; user can activate from workspaces page
    }
    setCloningWorkspaceId(workspaceId);
    setStep("cloning");
    // Immediately fetch so the new workspace appears in the list before the
    // polling interval fires — otherwise the effect finds ws=undefined and bails.
    refetchWorkspaces();
  };

  // ── Cloning: watch for the workspace to finish cloning then redirect ──
  useEffect(() => {
    if (step !== "cloning" || !cloningWorkspaceId) return;
    const ws = existingWorkspaces.find((w) => w.id === cloningWorkspaceId);
    if (!ws) return;
    if (ws.clone_error) {
      // Clone finished but not an Oxy project — go back with an error
      setError(ws.clone_error);
      setCloningWorkspaceId(null);
      setStep("pick");
      return;
    }
    if (!ws.is_cloning) {
      // Clone finished — run readiness checks before redirecting
      setStep("loading");
      Promise.all([
        OnboardingService.getReadiness(cloningWorkspaceId),
        WorkspaceService.getWorkspaceStatus(cloningWorkspaceId)
      ])
        .then(([readiness, status]) => {
          setMissingLlmKeys(readiness.llm_keys_missing ?? []);
          setRequiredSecrets(status.required_secrets ?? []);
          setStep("checks");
        })
        .catch(() => {
          window.location.replace("/");
        });
    }
  }, [step, cloningWorkspaceId, existingWorkspaces]);

  // ── Checks ──
  if (step === "checks") {
    const allGood = missingLlmKeys.length === 0 && requiredSecrets.length === 0;
    const isDemo = workspaceType === "demo";
    return (
      <div className='flex min-h-screen w-full flex-col items-center justify-center bg-background p-6'>
        <div className='w-full max-w-sm'>
          <div className='mb-8 text-center'>
            <div className='mb-4 flex justify-center'>
              <img src='/oxy-light.svg' alt='Oxy' className='dark:hidden' />
              <img src='/oxy-dark.svg' alt='Oxy' className='hidden dark:block' />
            </div>
            <h1 className='font-bold text-xl tracking-tight'>
              {allGood ? "Workspace ready!" : "Almost ready"}
            </h1>
            <p className='mt-1.5 text-muted-foreground text-sm'>
              {isDemo
                ? "Your demo workspace is ready to explore."
                : allGood
                  ? "Your workspace has been created successfully."
                  : "A few things need to be configured before you can use the workspace."}
            </p>
          </div>

          <div className='mb-6 flex flex-col gap-3'>
            {/* Missing LLM keys warning */}
            {missingLlmKeys.length > 0 && (
              <div className='rounded-lg border border-warning/20 bg-warning/5 p-4'>
                <div className='mb-2 flex items-center gap-2'>
                  <AlertTriangle className='h-4 w-4 shrink-0 text-warning' />
                  <p className='font-medium text-sm'>Missing LLM API keys</p>
                </div>
                <p className='mb-2 text-muted-foreground text-xs'>
                  Set these environment variables before starting the server:
                </p>
                <ul className='flex flex-col gap-1'>
                  {missingLlmKeys.map((key) => (
                    <li key={key} className='font-mono text-[11px] text-foreground'>
                      {key}
                    </li>
                  ))}
                </ul>
              </div>
            )}

            {/* Missing database secrets warning */}
            {requiredSecrets.length > 0 && (
              <div className='rounded-lg border border-warning/20 bg-warning/5 p-4'>
                <div className='mb-2 flex items-center gap-2'>
                  <AlertTriangle className='h-4 w-4 shrink-0 text-warning' />
                  <p className='font-medium text-sm'>Required database secrets</p>
                </div>
                <p className='mb-2 text-muted-foreground text-xs'>
                  Add these secrets in Settings → Secrets:
                </p>
                <ul className='flex flex-col gap-1'>
                  {requiredSecrets.map((secret) => (
                    <li key={secret} className='font-mono text-[11px] text-foreground'>
                      {secret}
                    </li>
                  ))}
                </ul>
              </div>
            )}

            {/* Setup hints — always shown */}
            <div className='flex flex-col gap-2'>
              <div className='flex items-start gap-3 rounded-lg border border-border bg-muted/30 px-3.5 py-3'>
                <div className='mt-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-md border border-border bg-background'>
                  <Key className='h-3 w-3 text-muted-foreground' />
                </div>
                <div className='min-w-0 flex-1'>
                  <p className='font-medium text-[13px]'>LLM API key</p>
                  <p className='mt-0.5 text-muted-foreground text-xs leading-relaxed'>
                    Add your LLM provider key in Settings → Secrets.
                  </p>
                </div>
                {cloningWorkspaceId && (
                  <a
                    href={ROUTES.WORKSPACE(cloningWorkspaceId).IDE.SETTINGS.SECRETS}
                    className='mt-0.5 shrink-0 text-primary text-xs hover:underline'
                  >
                    Set up ↗
                  </a>
                )}
              </div>
              <div className='flex items-start gap-3 rounded-lg border border-border bg-muted/30 px-3.5 py-3'>
                <div className='mt-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-md border border-border bg-background'>
                  <Database className='h-3 w-3 text-muted-foreground' />
                </div>
                <div className='min-w-0 flex-1'>
                  {isDemo ? (
                    <>
                      <p className='font-medium text-[13px]'>Sample data included</p>
                      <p className='mt-0.5 text-muted-foreground text-xs leading-relaxed'>
                        Pre-loaded DuckDB databases with retail and sales data. No setup required.
                      </p>
                    </>
                  ) : (
                    <>
                      <p className='font-medium text-[13px]'>Database connection</p>
                      <p className='mt-0.5 text-muted-foreground text-xs leading-relaxed'>
                        Connect a database so agents can run SQL queries.
                      </p>
                    </>
                  )}
                </div>
                {!isDemo && cloningWorkspaceId && (
                  <a
                    href={ROUTES.WORKSPACE(cloningWorkspaceId).IDE.SETTINGS.DATABASES}
                    className='mt-0.5 shrink-0 text-primary text-xs hover:underline'
                  >
                    Set up ↗
                  </a>
                )}
              </div>
            </div>
          </div>

          <button
            type='button'
            onClick={() => window.location.replace("/")}
            className='flex w-full items-center justify-center gap-2 rounded-lg bg-primary px-4 py-2.5 font-medium text-primary-foreground text-sm transition-opacity hover:opacity-90'
          >
            <CheckCircle2 className='h-4 w-4' />
            Enter workspace
          </button>
          <p className='mt-3 text-center text-muted-foreground text-xs'>
            You can update these settings anytime in the IDE.
          </p>
        </div>
      </div>
    );
  }

  // ── Loading ──
  if (step === "loading") {
    return (
      <div className='flex min-h-screen w-full items-center justify-center bg-background'>
        <div className='flex flex-col items-center gap-4'>
          <Loader2 className='h-8 w-8 animate-spin text-primary' />
          <p className='text-muted-foreground text-sm'>Setting up your workspace…</p>
        </div>
      </div>
    );
  }

  // ── Cloning ──
  if (step === "cloning") {
    return (
      <div className='flex min-h-screen w-full items-center justify-center bg-background'>
        <div className='flex w-full max-w-sm flex-col items-center gap-6 p-6 text-center'>
          <div className='flex flex-col items-center gap-3'>
            <Loader2 className='h-8 w-8 animate-spin text-primary' />
            <p className='font-medium text-sm'>Cloning repository…</p>
            <p className='text-muted-foreground text-xs'>
              This may take a moment for large repositories. You'll be redirected automatically when
              it's done.
            </p>
          </div>
          <button
            type='button'
            onClick={() => window.location.replace("/")}
            className='text-muted-foreground text-xs underline underline-offset-2 hover:text-foreground'
          >
            Continue to app anyway
          </button>
        </div>
      </div>
    );
  }

  // ── Blank workspace ──
  if (step === "new") {
    return (
      <div className='flex min-h-screen w-full flex-col items-center justify-center bg-background p-6'>
        <div className='w-full max-w-sm'>
          <button
            type='button'
            onClick={() => {
              setError(null);
              setStep("pick");
            }}
            className='mb-6 flex items-center gap-1.5 text-muted-foreground text-sm transition-colors hover:text-foreground'
          >
            <ArrowLeft className='h-3.5 w-3.5' />
            Back
          </button>

          <div className='mb-8'>
            <h2 className='font-semibold text-xl tracking-tight'>Create blank workspace</h2>
            <p className='mt-1.5 text-muted-foreground text-sm'>
              Start from scratch with an empty workspace.
            </p>
          </div>

          <div className='flex flex-col gap-4'>
            <div className='space-y-1.5'>
              <Label htmlFor='new-workspace-name'>Workspace name</Label>
              <Input
                id='new-workspace-name'
                placeholder='my-workspace'
                value={workspaceName}
                onChange={(e) => setWorkspaceName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && !nameConflict) handleNew();
                }}
                autoFocus
              />
              {nameConflict ? (
                <p className='text-destructive text-xs'>
                  Name already taken — use{" "}
                  <button
                    type='button'
                    className='font-mono underline underline-offset-2 hover:no-underline'
                    onClick={() => setWorkspaceName(nameConflict)}
                  >
                    {nameConflict}
                  </button>{" "}
                  instead?
                </p>
              ) : (
                <p className='text-muted-foreground text-xs'>Leave blank for a default name.</p>
              )}
            </div>

            {error && (
              <p className='rounded-md border border-destructive/20 bg-destructive/5 px-3 py-2 text-center text-destructive text-sm'>
                {error}
              </p>
            )}

            <button
              type='button'
              onClick={handleNew}
              disabled={nameConflict !== null}
              className='flex w-full items-center justify-center gap-2 rounded-lg bg-primary px-4 py-2.5 font-medium text-primary-foreground text-sm transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-40'
            >
              Create workspace
            </button>
          </div>
        </div>
      </div>
    );
  }

  // ── GitHub import ──
  if (step === "github") {
    return (
      <div className='flex min-h-screen w-full flex-col items-center justify-center bg-background p-6'>
        <div className='w-full max-w-md'>
          <button
            type='button'
            onClick={() => setStep("pick")}
            className='mb-6 flex items-center gap-1.5 text-muted-foreground text-sm transition-colors hover:text-foreground'
          >
            <ArrowLeft className='h-3.5 w-3.5' />
            Back
          </button>
          <h2 className='mb-6 font-semibold text-xl tracking-tight'>Import from GitHub</h2>
          {isLocal ? (
            // Local mode: URL + system credentials, PAT fallback
            <GitHubUrlOnboardingStep
              workspaceName={nameForApi}
              onBack={() => setStep("pick")}
              onDone={handleGitHubDone}
            />
          ) : (
            // Multi-workspace mode: GitHub App / namespace picker
            <GitHubOnboardingStep
              projectName={nameForApi}
              onBack={() => setStep("pick")}
              onDone={handleGitHubDone}
            />
          )}
        </div>
      </div>
    );
  }

  // ── Main pick screen ──
  return (
    <div className='flex min-h-screen w-full flex-col items-center justify-center bg-background p-6'>
      {/* Header */}
      <div className='mb-10 text-center'>
        <div className='mb-4 flex justify-center'>
          <img src='/oxy-light.svg' alt='Oxy' className='dark:hidden' />
          <img src='/oxy-dark.svg' alt='Oxy' className='hidden dark:block' />
        </div>
        <h1 className='font-bold text-2xl tracking-tight'>Welcome to Oxy</h1>
        <p className='mt-1.5 text-muted-foreground text-sm'>
          {hasExisting
            ? "Open an existing workspace or create a new one."
            : "Set up your first workspace to get started."}
        </p>
      </div>

      {/* Content — two columns when workspaces exist, single column otherwise */}
      {hasExisting ? (
        <div className='flex w-full max-w-3xl items-start gap-10'>
          {/* Left: existing workspaces */}
          <div className='flex min-w-0 flex-1 flex-col'>
            <p className='mb-3 font-medium text-[11px] text-muted-foreground uppercase tracking-widest'>
              Your workspaces
            </p>
            <div className='flex flex-col gap-0.5'>
              {existingWorkspaces.map((workspace) => (
                <ExistingWorkspaceRow key={workspace.id} workspace={workspace} />
              ))}
            </div>
          </div>

          {/* Divider */}
          <div className='mt-1 w-px self-stretch bg-border/50' />

          {/* Right: create new */}
          <div className='flex min-w-0 flex-1 flex-col gap-4'>
            <p className='font-medium text-[11px] text-muted-foreground uppercase tracking-widest'>
              New workspace
            </p>

            <div className='space-y-1'>
              <Label htmlFor='workspace-name-split' className='text-xs'>
                Workspace name
              </Label>
              <Input
                id='workspace-name-split'
                placeholder='my-workspace'
                value={workspaceName}
                onChange={(e) => setWorkspaceName(e.target.value)}
                className='font-mono text-sm'
              />
              {nameConflict ? (
                <p className='text-destructive text-xs'>
                  Name already taken — use{" "}
                  <button
                    type='button'
                    className='font-mono underline underline-offset-2 hover:no-underline'
                    onClick={() => setWorkspaceName(nameConflict)}
                  >
                    {nameConflict}
                  </button>{" "}
                  instead?
                </p>
              ) : (
                <p className='text-muted-foreground text-xs'>Leave blank for a default name.</p>
              )}
            </div>

            <div className='flex flex-col gap-2'>
              <CreateOption
                icon={<GithubIcon className='h-4 w-4' />}
                title='Import from GitHub'
                description='Clone an existing repository.'
                recommended
                disabled={nameConflict !== null}
                onClick={() => setStep("github")}
              />
              <CreateOption
                icon={<BookOpen className='h-4 w-4' />}
                title='Demo Workspace'
                description='Pre-built sample data and example queries.'
                disabled={nameConflict !== null}
                onClick={handleDemo}
              />
              <CreateOption
                icon={<Plus className='h-4 w-4' />}
                title='Blank Workspace'
                description='Start from scratch.'
                onClick={() => {
                  setWorkspaceName("");
                  setError(null);
                  setStep("new");
                }}
              />
            </div>

            {error && (
              <p className='rounded-md border border-destructive/20 bg-destructive/5 px-3 py-2 text-center text-destructive text-sm'>
                {error}
              </p>
            )}
          </div>
        </div>
      ) : (
        /* Single column when no existing workspaces */
        <div className='w-full max-w-sm'>
          <div className='mb-4 space-y-1'>
            <Label htmlFor='workspace-name'>Workspace name</Label>
            <Input
              id='workspace-name'
              placeholder='my-workspace'
              value={workspaceName}
              onChange={(e) => setWorkspaceName(e.target.value)}
            />
            {nameConflict ? (
              <p className='text-destructive text-xs'>
                Name already taken — use{" "}
                <button
                  type='button'
                  className='font-mono underline underline-offset-2 hover:no-underline'
                  onClick={() => setWorkspaceName(nameConflict)}
                >
                  {nameConflict}
                </button>{" "}
                instead?
              </p>
            ) : (
              <p className='text-muted-foreground text-xs'>Leave blank for a default name.</p>
            )}
          </div>

          <div className='flex flex-col gap-2'>
            <CreateOption
              icon={<GithubIcon className='h-4 w-4' />}
              title='Import from GitHub'
              description='Connect an existing repository and start working immediately.'
              recommended
              disabled={nameConflict !== null}
              onClick={() => setStep("github")}
            />
            <CreateOption
              icon={<BookOpen className='h-4 w-4' />}
              title='Demo Workspace'
              description='Explore Oxy with pre-built sample data and example queries.'
              disabled={nameConflict !== null}
              onClick={handleDemo}
            />
            <CreateOption
              icon={<Plus className='h-4 w-4' />}
              title='New Workspace'
              description='Start from a blank workspace and build from scratch.'
              onClick={() => {
                setWorkspaceName("");
                setError(null);
                setStep("new");
              }}
            />
          </div>

          {error && <p className='mt-3 text-center text-destructive text-sm'>{error}</p>}

          <p className='mt-8 text-center text-muted-foreground text-xs'>
            You can connect to GitHub or change settings later.
          </p>
        </div>
      )}
    </div>
  );
}
