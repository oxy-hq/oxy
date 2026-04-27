import { AlertCircle, ArrowRight, Database, GitFork, X } from "lucide-react";
import { useState } from "react";
import { Link, Navigate, useLocation } from "react-router-dom";
import ChatPanel from "@/components/Chat/ChatPanel";
import PageHeader from "@/components/PageHeader";
import useSidebar from "@/components/ui/shadcn/sidebar-context";
import { hasPendingOnboardingForWorkspace } from "@/components/workspaces/components/CreateWorkspaceDialog/components/orchestrator";
import useAgents from "@/hooks/api/agents/useAgents";
import useDatabases from "@/hooks/api/databases/useDatabases";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { cn } from "@/libs/shadcn/utils";
import ROUTES from "@/libs/utils/routes";
import useCurrentOrg from "@/stores/useCurrentOrg";

const getGreeting = () => {
  const hour = new Date().getHours();
  if (hour < 12) return "Good Morning";
  if (hour < 18) return "Good Afternoon";
  return "Good Evening";
};

const ProjectSetupToast = () => {
  const { project } = useCurrentProjectBranch();
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";
  const { data: agents = [], isSuccess: agentsLoaded } = useAgents();
  const { data: databases = [], isSuccess: dbLoaded } = useDatabases();
  const [dismissed, setDismissed] = useState(false);

  const hasAgents = agents.filter((a) => a.public).length > 0;
  const hasDatabases = databases.length > 0;

  if (!agentsLoaded || !dbLoaded) return null;
  if (hasAgents && hasDatabases) return null;
  if (dismissed) return null;

  const routes = ROUTES.ORG(orgSlug).WORKSPACE(project.id);

  return (
    <div className='fade-in slide-in-from-top-2 fixed top-4 right-4 z-50 w-80 animate-in duration-300'>
      <div className='rounded-lg border border-amber-500/30 bg-background shadow-black/10 shadow-lg'>
        {/* Header */}
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

        {/* Items */}
        <div className='flex flex-col gap-1 p-2'>
          {!hasDatabases && (
            <div className='flex items-center justify-between rounded-md px-2 py-2'>
              <div className='flex items-center gap-2 text-muted-foreground text-xs'>
                <Database className='h-3 w-3 shrink-0' />
                No database connection
              </div>
              <Link
                to={routes.IDE.SETTINGS.DATABASES}
                className='flex items-center gap-1 font-medium text-primary text-xs hover:underline'
              >
                Configure
                <ArrowRight className='h-3 w-3' />
              </Link>
            </div>
          )}
          {!hasAgents && (
            <div className='flex items-center justify-between rounded-md px-2 py-2'>
              <div className='flex items-center gap-2 text-muted-foreground text-xs'>
                <GitFork className='h-3 w-3 shrink-0' />
                No agents configured
              </div>
              <Link
                to={routes.IDE.ROOT}
                className='flex items-center gap-1 font-medium text-primary text-xs hover:underline'
              >
                Open IDE
                <ArrowRight className='h-3 w-3' />
              </Link>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};

const Home = () => {
  const { open } = useSidebar();
  const { project } = useCurrentProjectBranch();
  const { data: agents = [], isSuccess: agentsLoaded } = useAgents();
  const { data: databases = [], isSuccess: dbLoaded } = useDatabases();
  const location = useLocation();
  const locationState = location.state as {
    prefillQuestion?: string;
    agentPath?: string;
    autoSubmit?: boolean;
  } | null;

  // If onboarding is mid-flight for this workspace, bounce back to /onboarding
  // instead of rendering the chat. This covers the case where a blank
  // workspace was created but its agentic setup never reached "complete".
  if (project?.id && hasPendingOnboardingForWorkspace(project.id)) {
    return <Navigate to='onboarding' replace />;
  }

  const greeting = getGreeting();

  const setupComplete =
    !agentsLoaded ||
    !dbLoaded ||
    (agents.filter((a) => a.public).length > 0 && databases.length > 0);

  return (
    <div className='flex h-full flex-col'>
      {!open && <PageHeader />}
      <ProjectSetupToast />
      <div className='flex h-full flex-col items-center justify-center gap-10 px-4'>
        <p className='text-center text-3xl'>{greeting}! How can I assist you?</p>

        <div className='flex w-full max-w-4xl flex-col items-center gap-3 pb-40'>
          {!setupComplete && (
            <p className='text-center text-muted-foreground/50 text-xs'>
              Complete the setup steps above to start chatting.
            </p>
          )}
          <div
            className={cn("w-full", !setupComplete && "pointer-events-none select-none opacity-40")}
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
