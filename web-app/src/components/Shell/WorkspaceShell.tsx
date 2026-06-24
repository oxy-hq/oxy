import { Workflow as Automation, Globe, House, MessagesSquare } from "lucide-react";
import type { ReactNode } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { AskPanel } from "@/components/Ask/AskPanel";
import { AskPill } from "@/components/Ask/AskPill";
import { OxyCoreMark } from "@/components/OxyCoreMark";
import WorkspaceStatus from "@/components/WorkspaceStatus";
import { useCustomApps } from "@/hooks/api/customApps/useCustomApps";
import ROUTES from "@/libs/utils/routes";
import useCurrentOrg from "@/stores/useCurrentOrg";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";
import { RailUserMenu } from "./RailUserMenu";
import { RailWorkspaceSwitch } from "./RailWorkspaceSwitch";
import { RailWorkspaceTile } from "./RailWorkspaceTile";
import { type RailItem, ShellRail } from "./ShellRail";

/** The workspace chrome: icon rail + content column. Wraps every
 *  workspace route. The rail hides inside Oxygen Factory / the IDE (it has its
 *  own chrome) and on the onboarding wizard; the content column is identical
 *  everywhere so pages never re-layout between routes. */
export function WorkspaceShell({ children }: { children: ReactNode }) {
  const location = useLocation();
  const navigate = useNavigate();
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";
  const { workspace } = useCurrentWorkspace();
  const wsId = workspace?.id ?? "";
  const ws = ROUTES.ORG(orgSlug).WORKSPACE(wsId);
  const { data: customApps = [] } = useCustomApps(wsId);

  const path = location.pathname;
  const inIde = /\/ide(\/|$)/.test(path);
  const hideRail = inIde || /\/onboarding(\/|$)/.test(path);
  // The IDE renders its own ProjectStatus — don't double-banner it.
  const hideStatus = inIde;

  // In local mode ws.ROOT is "" — the index route is "/".
  const isHome = path === ws.HOME || path === (ws.ROOT || "/");

  // Conceptual groups (divided in the rail): HQ · Apps · Intelligence.
  const hq: RailItem = {
    key: "hq",
    label: "HQ",
    testId: "rail-hq",
    icon: <House className='h-4 w-4' />,
    active: isHome,
    onSelect: () => navigate(ws.HOME)
  };
  const appItems: RailItem[] = customApps.map((app) => ({
    key: app.id,
    label: app.name,
    testId: `rail-app-${app.slug}`,
    letter: app.name.slice(0, 1).toUpperCase(),
    imageUrl: app.icon_url,
    href: app.url
  }));
  const intelligence: RailItem[] = [
    {
      key: "threads",
      label: "Threads",
      testId: "rail-threads",
      icon: <MessagesSquare className='h-4 w-4' />,
      active: path.startsWith(ws.THREADS),
      onSelect: () => navigate(ws.THREADS)
    },
    {
      key: "automations",
      label: "Automations",
      testId: "rail-automations",
      icon: <Automation className='h-4 w-4' />,
      active: path.startsWith(ws.WORKFLOWS),
      onSelect: () => navigate(ws.WORKFLOWS)
    },
    {
      key: "world-model",
      label: "World Model",
      testId: "rail-world-model",
      icon: <Globe className='h-4 w-4' />,
      active: path.startsWith(ws.WORLD_MODEL),
      onSelect: () => navigate(ws.WORLD_MODEL)
    }
  ];
  // System: the intelligence substrate powering the HQ. Pinned at the
  // bottom of the nav, distinct from the operator apps above.
  const core: RailItem = {
    key: "core",
    label: "Oxygen Factory",
    tooltip:
      "Oxygen Factory — the intelligence system behind your HQ: data sources, business model, agents, automations, and deployments",
    testId: "rail-core",
    icon: <OxyCoreMark className='h-6 w-6' />,
    active: path.startsWith(ws.IDE.ROOT),
    onSelect: () => navigate(ws.IDE.ROOT)
  };

  // Apps only get their own divided group when the workspace has any.
  const groups: RailItem[][] = appItems.length
    ? [[hq], appItems, intelligence]
    : [[hq], intelligence];

  return (
    <div className='flex h-full w-full'>
      {!hideRail && (
        <ShellRail
          top={<RailWorkspaceTile />}
          groups={groups}
          footerItems={[core]}
          bottom={
            <>
              <RailWorkspaceSwitch />
              <RailUserMenu />
            </>
          }
        />
      )}
      <main className='relative flex h-full min-w-0 flex-1 flex-col bg-background'>
        {!hideStatus && <WorkspaceStatus />}
        <div className='w-full min-w-0 flex-1 overflow-hidden'>{children}</div>
        {/* Mounted inside the relative <main> so they center on content,
            not the viewport (the rail would skew viewport-centering). */}
        <AskPill />
        <AskPanel />
      </main>
    </div>
  );
}
