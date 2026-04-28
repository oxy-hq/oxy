import { Check, HardDrive, LogOut, Plus, Settings, UserPlus } from "lucide-react";
import { useEffect, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import OrgSettingsDialog, { type OrgSettingsTab } from "@/components/org/OrgSettingsDialog";
import { useAuth } from "@/contexts/AuthContext";
import { useOrgs } from "@/hooks/api/organizations";
import { cn } from "@/libs/shadcn/utils";
import ROUTES from "@/libs/utils/routes";
import useCurrentOrg from "@/stores/useCurrentOrg";
import type { UserInfo } from "@/types/auth";
import type { Organization } from "@/types/organization";
import { UserAvatar } from "../UserAvatar";
import { Button } from "../ui/shadcn/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger
} from "../ui/shadcn/dropdown-menu";
import { Tooltip, TooltipContent, TooltipTrigger } from "../ui/shadcn/tooltip";

function orgStatsLabel(org: Organization) {
  const members = org.member_count ?? 0;
  const workspaces = org.workspace_count ?? 0;
  const memberText = `${members} ${members === 1 ? "member" : "members"}`;
  const workspaceText = `${workspaces} ${workspaces === 1 ? "workspace" : "workspaces"}`;
  return `${memberText} · ${workspaceText}`;
}

// Intentionally distinct from `@/hooks/api/users/useCurrentUser` — that hook
// queries the server, whereas this one reads the cached user blob from auth
// storage so the footer can render without waiting for a network round-trip.
function useLocalUserInfo(): UserInfo | null {
  const { getUser } = useAuth();
  try {
    return JSON.parse(getUser() || "null");
  } catch {
    return null;
  }
}

function LocalModeFooter() {
  return (
    <div className='border-sidebar-border/50 border-t p-2'>
      <div className='flex items-center gap-2.5 rounded-md px-2 py-2 text-left group-data-[collapsible=icon]:justify-center group-data-[collapsible=icon]:px-0'>
        <div className='flex h-6 w-6 shrink-0 items-center justify-center rounded-lg bg-muted text-muted-foreground'>
          <HardDrive className='h-4 w-4' />
        </div>
        <div className='grid min-w-0 flex-1 leading-tight group-data-[collapsible=icon]:hidden'>
          <span className='truncate font-medium text-[13px] text-sidebar-foreground'>
            Local mode
          </span>
          <span className='truncate text-[11px] text-muted-foreground'>
            Running against local config
          </span>
        </div>
      </div>
    </div>
  );
}

export function Footer() {
  const { isLocalMode } = useAuth();
  if (isLocalMode) {
    return <LocalModeFooter />;
  }
  return <CloudFooter />;
}

function CloudFooter() {
  const navigate = useNavigate();
  const { logout } = useAuth();
  const currentUser = useLocalUserInfo();
  const { org: currentOrg } = useCurrentOrg();
  const role = useCurrentOrg((s) => s.role);
  const { data: orgs } = useOrgs();
  const [menuOpen, setMenuOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsTab, setSettingsTab] = useState<OrgSettingsTab>("general");
  const [searchParams, setSearchParams] = useSearchParams();

  const isAdmin = role === "owner" || role === "admin";

  // After a successful Slack install, the backend redirects the browser
  // to /<orgSlug>?slack_installed=ok. Detect the param, surface a toast,
  // pop open the settings dialog on the Integration tab, and strip the
  // param so a refresh doesn't re-fire the toast.
  useEffect(() => {
    if (searchParams.get("slack_installed") !== "ok") return;
    toast.success("Slack connected");
    setSettingsTab("integration");
    setSettingsOpen(true);
    const next = new URLSearchParams(searchParams);
    next.delete("slack_installed");
    setSearchParams(next, { replace: true });
  }, [searchParams, setSearchParams]);

  const openSettings = (tab: OrgSettingsTab) => {
    setSettingsTab(tab);
    setSettingsOpen(true);
    setMenuOpen(false);
  };

  const email = currentUser?.email ?? "guest@oxy.local";
  const name = currentUser?.name ?? "";
  const picture = currentUser?.picture;
  const isGuest = !currentUser;
  const showLogout = !isGuest;

  const displayOrg = orgs?.find((o) => o.id === currentOrg?.id) ?? currentOrg;
  const statsLabel = displayOrg ? orgStatsLabel(displayOrg) : "";

  const handleSwitchOrg = (org: Organization) => {
    navigate(ROUTES.ORG(org.slug).ROOT);
  };

  return (
    <div className='border-sidebar-border/50 border-t p-2'>
      <DropdownMenu open={menuOpen} onOpenChange={setMenuOpen}>
        <DropdownMenuTrigger asChild>
          <Button
            variant='ghost'
            className='flex h-auto w-full items-center gap-2.5 px-2 py-1.5 group-data-[collapsible=icon]:justify-center group-data-[collapsible=icon]:px-0'
          >
            <UserAvatar
              name={name}
              email={email}
              picture={picture}
              className='h-6 w-6 shrink-0 rounded'
            />
            <div className='grid min-w-0 flex-1 text-left leading-tight group-data-[collapsible=icon]:hidden'>
              <span className='truncate font-medium text-[13px] text-sidebar-foreground'>
                {name || email.split("@")[0]}
              </span>
              <span className='truncate text-[11px] text-muted-foreground'>{email}</span>
            </div>
          </Button>
        </DropdownMenuTrigger>

        <DropdownMenuContent
          side='top'
          align='start'
          sideOffset={4}
          className='min-w-64 rounded-lg'
        >
          {displayOrg && (
            <>
              <div className='flex items-center gap-2.5 px-2 py-2'>
                <div className='flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-primary font-bold text-primary-foreground text-sm'>
                  {displayOrg.name[0]?.toUpperCase()}
                </div>
                <div className='grid min-w-0 flex-1 leading-tight'>
                  <span className='truncate font-medium text-sm'>{displayOrg.name}</span>
                  <span className='truncate text-[11px] text-muted-foreground'>{statsLabel}</span>
                </div>
              </div>
              {isAdmin && (
                <div className='flex gap-1.5 px-2 pb-2'>
                  <Button variant='outline' size='sm' onClick={() => openSettings("general")}>
                    <Settings />
                    Settings
                  </Button>
                  <Button variant='outline' size='sm' onClick={() => openSettings("team")}>
                    <UserPlus />
                    Invite members
                  </Button>
                </div>
              )}
              <DropdownMenuSeparator />
            </>
          )}

          <DropdownMenuGroup className='flex flex-col gap-1'>
            {orgs?.map((org) => (
              <Tooltip key={org.id} delayDuration={300}>
                <TooltipTrigger asChild>
                  <DropdownMenuItem
                    onClick={() => handleSwitchOrg(org)}
                    className={cn(
                      "flex cursor-pointer items-center gap-2",
                      currentOrg?.id === org.id && "bg-muted"
                    )}
                  >
                    <div className='flex h-6 w-6 items-center justify-center rounded bg-primary/10 font-bold text-primary text-xs'>
                      {org.name[0]?.toUpperCase()}
                    </div>
                    <span className='flex-1 truncate'>{org.name}</span>
                    {currentOrg?.id === org.id && <Check className='h-4 w-4 text-primary' />}
                  </DropdownMenuItem>
                </TooltipTrigger>
                <TooltipContent
                  className='max-w-56 bg-card p-3'
                  arrowClassName='bg-card fill-card'
                  side='right'
                  sideOffset={8}
                >
                  <p className='text-muted-foreground text-xs'>{orgStatsLabel(org)}</p>
                </TooltipContent>
              </Tooltip>
            ))}
            <DropdownMenuItem
              className='cursor-pointer'
              onClick={() => {
                setMenuOpen(false);
                navigate(ROUTES.ONBOARDING);
              }}
            >
              <Plus className='h-4 w-4' />
              New organization
            </DropdownMenuItem>
          </DropdownMenuGroup>

          {showLogout && (
            <>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                className='cursor-pointer text-destructive focus:text-destructive'
                onClick={logout}
              >
                <LogOut className='h-4 w-4' />
                <span>Log out</span>
              </DropdownMenuItem>
            </>
          )}
        </DropdownMenuContent>
      </DropdownMenu>

      {displayOrg && role && (
        <OrgSettingsDialog
          open={settingsOpen}
          onOpenChange={setSettingsOpen}
          org={displayOrg}
          viewerRole={role}
          defaultTab={settingsTab}
        />
      )}
    </div>
  );
}
