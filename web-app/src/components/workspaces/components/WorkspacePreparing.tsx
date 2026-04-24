import { useQueryClient } from "@tanstack/react-query";
import {
  AlertTriangle,
  CheckCircle2,
  FileWarning,
  GitBranch,
  Loader2,
  Sparkles
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import type { WorkspaceCreationType } from "@/components/workspaces/types";
import queryKeys from "@/hooks/api/queryKey";
import { useAllWorkspaces } from "@/hooks/api/workspaces/useWorkspaces";
import { cn } from "@/libs/shadcn/utils";
import { setLastWorkspaceId } from "@/libs/utils/lastWorkspace";
import ROUTES from "@/libs/utils/routes";
import { initOnboardingStateForWorkspace } from "./CreateWorkspaceDialog/components/orchestrator";

const REDIRECT_SECONDS = 5;

/**
 * Post-creation holding screen. A freshly-created workspace isn't always
 * usable immediately — GitHub imports clone in the background, and even
 * local demo/new writes briefly touch the filesystem — so we poll the list
 * endpoint (`useAllWorkspaces` already refetches every 3 s while something
 * is `cloning`) and only navigate the user into the IDE once the backend
 * reports `status === "ready"`.
 *
 *   ready                → persist last-opened, navigate to workspace home
 *   cloning / unknown    → loader copy tailored to the creation type
 *   failed               → show the backend error with a retry-from-start button
 *   not_oxy_project      → clone succeeded but no config.yml at root; offer an
 *                          "Open in IDE" button so the user can add one
 *                          (retrying the clone would hit the same outcome)
 */
export default function WorkspacePreparing({
  workspaceId,
  creationType,
  orgId,
  orgSlug,
  onRetry
}: {
  workspaceId: string;
  creationType: WorkspaceCreationType;
  orgId: string;
  orgSlug: string;
  onRetry: () => void;
}) {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { data: workspaces } = useAllWorkspaces(orgId);

  // The list query was cached before this workspace existed — invalidate once
  // on mount so the first poll includes the new entry.
  useEffect(() => {
    queryClient.invalidateQueries({ queryKey: queryKeys.workspaces.list() });
  }, [queryClient]);

  const ws = workspaces?.find((w) => w.id === workspaceId);
  const status = ws?.status;

  // Blank ("new") and GitHub-imported workspaces both drop the user into the
  // agentic onboarding flow — blank walks the full model/warehouse/build arc,
  // github mode only collects the secrets referenced by the cloned repo's
  // config.yml. Demo workspaces skip onboarding since they ship with sample
  // agents + data already configured.
  const goToWorkspace = useCallback(() => {
    if (creationType === "new" || creationType === "github") {
      initOnboardingStateForWorkspace(workspaceId, creationType);
      navigate(ROUTES.ORG(orgSlug).WORKSPACE(workspaceId).ONBOARDING, { replace: true });
      return;
    }
    navigate(ROUTES.ORG(orgSlug).WORKSPACE(workspaceId).HOME, { replace: true });
  }, [creationType, navigate, orgSlug, workspaceId]);

  const [countdown, setCountdown] = useState(REDIRECT_SECONDS);

  // Even after the backend flips to "ready" we keep the preparing view on
  // screen until `PreparingView` has walked through every stage — otherwise
  // the user sees us abruptly skip from "connecting to GitHub" straight to
  // "ready", which feels like the stages were fake. The preparing view
  // speeds up when `forceComplete` is true so the tail doesn't drag once
  // the real work is finished.
  const [stagesDone, setStagesDone] = useState(false);

  // Persist last-opened as soon as the workspace is ready — independent of
  // whether the user lets the countdown run out or clicks "Go now".
  useEffect(() => {
    if (status === "ready") setLastWorkspaceId(orgId, workspaceId);
  }, [status, orgId, workspaceId]);

  useEffect(() => {
    if (status !== "ready") return;
    if (countdown <= 0) {
      goToWorkspace();
      return;
    }
    const timer = setTimeout(() => setCountdown((c) => c - 1), 1000);
    return () => clearTimeout(timer);
  }, [status, countdown, goToWorkspace]);

  if (status === "failed") {
    return (
      <div className='flex flex-col items-center gap-4 py-8 text-center'>
        <div className='flex size-12 items-center justify-center rounded-full bg-destructive/10'>
          <AlertTriangle className='size-6 text-destructive' />
        </div>
        <div className='flex flex-col gap-1'>
          <h3 className='font-semibold text-base'>Workspace setup failed</h3>
          <p className='max-w-sm text-muted-foreground text-sm'>
            {ws?.error ?? "Something went wrong while preparing your workspace."}
          </p>
        </div>
        <Button variant='outline' onClick={onRetry}>
          Try again
        </Button>
      </div>
    );
  }

  if (status === "not_oxy_project") {
    const openInIde = () => {
      setLastWorkspaceId(orgId, workspaceId);
      navigate(ROUTES.ORG(orgSlug).WORKSPACE(workspaceId).IDE.FILES.ROOT, { replace: true });
    };
    return (
      <div className='flex flex-col items-center gap-4 py-8 text-center'>
        <div className='flex size-12 items-center justify-center rounded-full bg-warning/10'>
          <FileWarning className='size-6 text-warning' />
        </div>
        <div className='flex flex-col gap-1'>
          <h3 className='font-semibold text-base'>Repository cloned, but not an Oxy project</h3>
          <p className='max-w-sm text-muted-foreground text-sm'>
            No <code className='rounded bg-muted px-1 py-0.5 text-xs'>config.yml</code> was found at
            the workspace root. Open the IDE to add one.
          </p>
        </div>
        <Button onClick={openInIde}>Open in IDE</Button>
      </div>
    );
  }

  if (status === "ready" && stagesDone) {
    return (
      <div className='flex flex-col items-center gap-3 py-10 text-center'>
        <CheckCircle2 className='size-8 text-primary' />
        <p className='text-muted-foreground text-sm'>
          Workspace ready. Redirecting in {countdown}…
        </p>
        <Button onClick={goToWorkspace}>Go now</Button>
      </div>
    );
  }

  // status === "cloning" | "ready" (stages still playing out) | undefined
  return (
    <PreparingView
      creationType={creationType}
      forceComplete={status === "ready"}
      onAllStagesComplete={() => setStagesDone(true)}
    />
  );
}

/**
 * The long-running preparation view. Only one stage is visible at a time:
 * the active stage shows a spinner, flips to a checkmark when it completes,
 * fades out, and the next stage fades in to replace it.
 *
 * Normal mode: stages advance on `stage.doneAt` timestamps (seconds since
 * mount) — these are fictional but give the user something moving.
 *
 * `forceComplete` mode (backend flipped to "ready" early): we stop the
 * elapsed-time driver and rip through the remaining stages with shorter
 * timings so the transition into the ready screen still *feels* staged but
 * doesn't drag. The last stage (`doneAt: null`) only resolves in this mode.
 * When the final stage finishes, `onAllStagesComplete` fires so the parent
 * can swap to the "ready" screen.
 */
type PreparingPhase = "loading" | "done" | "exiting";
const DONE_MS = 350;
const EXIT_MS = 120;
// In force-complete mode we still want each stage to be legible — don't shave
// the transitions so hard that stages blur past. These stay close to normal;
// the speed-up in catch-up comes from skipping the `doneAt` wait, not from
// rushing the animation itself. We also hold the spinner for
// `LOADING_MS_FAST` on each stage so the user sees it "work" before flipping
// to the checkmark, rather than every stage arriving pre-completed.
const LOADING_MS_FAST = 450;

function PreparingView({
  creationType,
  forceComplete,
  onAllStagesComplete
}: {
  creationType: WorkspaceCreationType;
  forceComplete: boolean;
  onAllStagesComplete: () => void;
}) {
  const { title, subtitle, stages, Icon } = viewConfig(creationType);

  const [stageIndex, setStageIndex] = useState(0);
  const [phase, setPhase] = useState<PreparingPhase>("loading");
  const [elapsed, setElapsed] = useState(0);

  // Drive elapsed-time only while we're waiting for real progress. Once the
  // backend has signaled ready we advance on timers, not on wall clock.
  useEffect(() => {
    if (forceComplete) return;
    const id = setInterval(() => setElapsed((e) => e + 1), 1000);
    return () => clearInterval(id);
  }, [forceComplete]);

  // Enter the "done" phase when the active stage is ready to complete.
  // In force-complete mode every stage still holds its spinner for
  // `LOADING_MS_FAST` before flipping — otherwise stages arrive already
  // checked and the user never sees them "work".
  useEffect(() => {
    if (phase !== "loading") return;
    const current = stages[stageIndex];
    if (!current) return;
    if (forceComplete) {
      const t = setTimeout(() => setPhase("done"), LOADING_MS_FAST);
      return () => clearTimeout(t);
    }
    const reachedByTime = current.doneAt !== null && elapsed >= current.doneAt;
    if (reachedByTime) setPhase("done");
  }, [phase, stageIndex, stages, elapsed, forceComplete]);

  // After the checkmark lingers, fade the stage out.
  useEffect(() => {
    if (phase !== "done") return;
    const t = setTimeout(() => setPhase("exiting"), DONE_MS);
    return () => clearTimeout(t);
  }, [phase]);

  // After the fade-out, advance to the next stage or signal completion.
  useEffect(() => {
    if (phase !== "exiting") return;
    const t = setTimeout(() => {
      if (stageIndex === stages.length - 1) {
        onAllStagesComplete();
      } else {
        setStageIndex((i) => i + 1);
        setPhase("loading");
      }
    }, EXIT_MS);
    return () => clearTimeout(t);
  }, [phase, stageIndex, stages.length, onAllStagesComplete]);

  const currentStage = stages[stageIndex];
  const showDone = phase === "done" || phase === "exiting";

  return (
    <div className='flex flex-col items-center gap-6 py-8'>
      <div className='relative flex size-20 items-center justify-center'>
        <span className='absolute inset-0 animate-ping rounded-full bg-primary/20 [animation-duration:2s]' />
        <span className='absolute inset-2 rounded-full bg-primary/15' />
        <span className='relative flex size-14 items-center justify-center rounded-full border border-primary/30 bg-primary/10 shadow-sm'>
          <Icon className='size-6 text-primary' />
        </span>
      </div>

      <div className='flex flex-col items-center gap-1 text-center'>
        <h3 className='font-semibold text-base'>{title}</h3>
        <p className='max-w-sm text-muted-foreground text-sm'>{subtitle}</p>
      </div>

      <div className='flex h-6 items-center justify-center'>
        <div
          key={stageIndex}
          className={cn(
            "flex items-center gap-2.5 text-sm transition-opacity ease-out",
            phase === "exiting"
              ? "opacity-0 duration-75"
              : "fade-in slide-in-from-bottom-1 animate-in fill-mode-both opacity-100 duration-150"
          )}
        >
          <span className='flex size-4 shrink-0 items-center justify-center'>
            {showDone ? (
              <CheckCircle2 className='size-4 text-primary' />
            ) : (
              <Loader2 className='size-4 animate-spin text-primary' />
            )}
          </span>
          <span className='text-foreground'>{currentStage.label}</span>
        </div>
      </div>

      <p className='text-muted-foreground/70 text-xs'>
        This usually takes a few seconds. You'll be taken in automatically.
      </p>
    </div>
  );
}

type Stage = { label: string; doneAt: number | null };

function viewConfig(type: WorkspaceCreationType): {
  title: string;
  subtitle: string;
  stages: Stage[];
  Icon: typeof GitBranch;
} {
  switch (type) {
    case "github":
      return {
        title: "Cloning your repository",
        subtitle: "Fetching files from GitHub and preparing the workspace.",
        Icon: GitBranch,
        stages: [
          { label: "Connecting to GitHub", doneAt: 2 },
          { label: "Cloning repository", doneAt: 8 },
          { label: "Indexing files", doneAt: 14 },
          { label: "Finalizing workspace", doneAt: null }
        ]
      };
    case "demo":
      return {
        title: "Preparing your demo workspace",
        subtitle: "Copying sample agents, procedures, and example queries.",
        Icon: Sparkles,
        stages: [
          { label: "Copying sample files", doneAt: 2 },
          { label: "Loading example queries", doneAt: 6 },
          { label: "Finalizing workspace", doneAt: null }
        ]
      };
    case "new":
      return {
        title: "Creating your workspace",
        subtitle: "Initializing the workspace directory and defaults.",
        Icon: Sparkles,
        stages: [
          { label: "Creating workspace directory", doneAt: 2 },
          { label: "Initializing configuration", doneAt: 5 },
          { label: "Finalizing workspace", doneAt: null }
        ]
      };
  }
}
