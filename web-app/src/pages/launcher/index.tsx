import { useEffect, useState } from "react";
import { Navigate, useLocation, useNavigate } from "react-router-dom";
import { workspaceLogoUrl } from "@/components/Shell/logoUrl";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useCustomApps } from "@/hooks/api/customApps/useCustomApps";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import useAskPanel from "@/stores/useAskPanel";
import useCurrentOrg from "@/stores/useCurrentOrg";
import { AppCard } from "./components/AppCard";
import { AskForwardFallback } from "./components/AskForwardFallback";
import { CriticalAlertBanner } from "./components/CriticalAlertBanner";
import { HqStatusLine } from "./components/HqStatusLine";
import { NeedsAttention } from "./components/NeedsAttention";
import { ProjectSetupToast } from "./components/ProjectSetupToast";
import { RecentActivity } from "./components/RecentActivity";
import useWorkspaceReadiness from "./useWorkspaceReadiness";

/** "Poke House HQ" in cloud, "HQ" in local mode (no org). */
function HqHeading({ workspaceId }: { workspaceId: string }) {
  const orgName = useCurrentOrg((s) => s.org?.name);
  const orgUpdatedAt = useCurrentOrg((s) => s.org?.updated_at);
  const [logoFailed, setLogoFailed] = useState(false);
  return (
    <div className='mb-2 flex items-center gap-3' data-testid='launcher-hq-heading'>
      {!logoFailed && workspaceId && (
        <img
          src={workspaceLogoUrl(workspaceId, orgUpdatedAt)}
          alt=''
          onError={() => setLogoFailed(true)}
          className='h-8 w-auto'
          data-testid='hq-logo'
        />
      )}
      <h1 className='font-semibold text-2xl'>{orgName ? `${orgName} HQ` : "HQ"}</h1>
    </div>
  );
}

const LauncherPage = () => {
  const readiness = useWorkspaceReadiness();
  const { project } = useCurrentProjectBranch();
  const { data: customApps = [], isPending: appsPending } = useCustomApps(project?.id ?? "");
  const location = useLocation();
  const navigate = useNavigate();
  const openAsk = useAskPanel((s) => s.open);

  // Back-compat with navigate(HOME, { state: { prefillQuestion, ... } })
  // callers (onboarding EXPLORE buttons etc.): open the Ask panel
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
          {/* Header + operational status (source freshness folded in) */}
          <div className='mx-auto w-full max-w-6xl px-6 pt-12 pb-8'>
            <HqHeading workspaceId={project?.id ?? ""} />
            <HqStatusLine />
          </div>
          {/* Critical alerts only — hidden in the calm default state */}
          <CriticalAlertBanner />
          {/* Apps — the primary operating surfaces, lead the page */}
          <div className='mx-auto w-full max-w-6xl px-6 pb-8'>
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
      <RecentActivity />
    </div>
  );
};

export default LauncherPage;
