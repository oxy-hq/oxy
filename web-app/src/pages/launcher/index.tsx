import { useEffect } from "react";
import { Navigate, useLocation, useNavigate } from "react-router-dom";
import { RecentThreads } from "@/components/RecentThreads";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useCustomApps } from "@/hooks/api/customApps/useCustomApps";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import useAskDock from "@/stores/useAskDock";
import { AppCard } from "./components/AppCard";
import { AskForwardFallback } from "./components/AskForwardFallback";
import { CriticalAlertBanner } from "./components/CriticalAlertBanner";
import { NeedsAttention } from "./components/NeedsAttention";
import { ProjectSetupToast } from "./components/ProjectSetupToast";
import useWorkspaceReadiness from "./useWorkspaceReadiness";

const LauncherPage = () => {
  const readiness = useWorkspaceReadiness();
  const { project } = useCurrentProjectBranch();
  const { data: customApps = [], isPending: appsPending } = useCustomApps(project?.id ?? "");
  const location = useLocation();
  const navigate = useNavigate();
  const openAsk = useAskDock((s) => s.open);

  // Back-compat with navigate(HOME, { state: { prefillQuestion, ... } })
  // callers (onboarding EXPLORE buttons etc.): open the Ask dock
  // prefilled instead of rendering an inline composer.
  const locationState = location.state as {
    prefillQuestion?: string;
    agentPath?: string;
    autoSubmit?: boolean;
  } | null;
  useEffect(() => {
    if (readiness.status !== "ready") return;
    if (!locationState?.prefillQuestion && !locationState?.agentPath) return;
    openAsk({
      message: locationState.prefillQuestion,
      agentPath: locationState.agentPath,
      autoSubmit: locationState.autoSubmit
    });
    // Consume the state so back/forward/refresh doesn't re-open or re-submit.
    navigate(location.pathname + location.search, { replace: true, state: null });
  }, [readiness.status, locationState, openAsk, navigate, location.pathname, location.search]);

  if (readiness.status === "loading" || appsPending) {
    return (
      <div className='flex h-full items-center justify-center'>
        <Spinner className='size-6' />
      </div>
    );
  }
  if (readiness.status === "redirect-onboarding") {
    return <Navigate to={readiness.to} replace />;
  }

  const hasApps = customApps.length > 0;

  return (
    <div className='flex h-full flex-col overflow-auto'>
      <ProjectSetupToast gaps={readiness.gaps} />
      {hasApps ? (
        <>
          {/* The HQ heading + status line moved to the universal top bar
              (breadcrumb "<Workspace> / HQ"). */}
          {/* Critical alerts only — hidden in the calm default state */}
          <CriticalAlertBanner />
          {/* Apps — the primary operating surfaces, lead the page */}
          <div className='mx-auto w-full max-w-6xl px-6 pt-12 pb-8'>
            <div
              className='grid grid-cols-1 gap-5 sm:grid-cols-2 xl:grid-cols-3'
              data-testid='launcher-app-grid'
            >
              {customApps.map((app) => (
                // art_url in the key remounts the card when art changes, resetting its error fallback
                <AppCard key={`${app.id}:${app.art_url ?? ""}`} app={app} />
              ))}
            </div>
          </div>
          {/* Needs attention — secondary intelligence module below the apps */}
          <NeedsAttention />
        </>
      ) : (
        <AskForwardFallback shouldDisableChat={readiness.shouldDisableChat} />
      )}
      <RecentThreads className='mx-auto w-full max-w-6xl px-6 pb-8' />
    </div>
  );
};

export default LauncherPage;
