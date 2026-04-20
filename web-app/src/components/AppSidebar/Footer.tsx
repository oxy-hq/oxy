import { Building2, Check, HardDrive, LogOut } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { useAuth } from "@/contexts/AuthContext";
import { useOrgs } from "@/hooks/api/organizations";
import { cn } from "@/libs/shadcn/utils";
import ROUTES from "@/libs/utils/routes";
import useCurrentOrg from "@/stores/useCurrentOrg";
import type { UserInfo } from "@/types/auth";
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

function useCurrentUser(): UserInfo | null {
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
  const currentUser = useCurrentUser();
  const { org: currentOrg, setOrg } = useCurrentOrg();
  const { data: orgs } = useOrgs();

  const email = currentUser?.email ?? "guest@oxy.local";
  const name = currentUser?.name ?? "";
  const picture = currentUser?.picture;
  const isGuest = !currentUser;
  const showLogout = !isGuest;

  const handleSwitchOrg = (org: NonNullable<typeof orgs>[number]) => {
    setOrg(org);
    navigate(ROUTES.ORG(org.slug).WORKSPACES);
  };

  return (
    <div className='border-sidebar-border/50 border-t p-2'>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant='ghost'
            className='flex w-full group-data-[collapsible=icon]:justify-center group-data-[collapsible=icon]:px-0'
          >
            <div className='flex h-6 w-6 shrink-0 items-center justify-center rounded-lg bg-primary font-bold text-primary-foreground text-sm'>
              {currentOrg?.name?.[0]?.toUpperCase() ?? "?"}
            </div>
            <div className='grid min-w-0 flex-1 text-left leading-tight group-data-[collapsible=icon]:hidden'>
              <span className='truncate font-medium text-[13px] text-sidebar-foreground'>
                {currentOrg?.name ?? "Select organization"}
              </span>
            </div>
          </Button>
        </DropdownMenuTrigger>

        <DropdownMenuContent
          side='top'
          align='start'
          sideOffset={4}
          className='w-[var(--radix-dropdown-menu-trigger-width)] min-w-56 rounded-lg'
        >
          {/* User info */}
          <div className='flex items-center gap-2.5 px-2 py-1.5'>
            <UserAvatar
              name={name}
              email={email}
              picture={picture}
              className='h-6 w-6 shrink-0 rounded'
            />
            <div className='grid min-w-0 flex-1 leading-tight'>
              <span className='truncate font-medium text-[13px]'>
                {name || email.split("@")[0]}
              </span>
              <span className='truncate text-[11px] text-muted-foreground'>{email}</span>
            </div>
          </div>
          <DropdownMenuSeparator />
          {/* Org list */}
          <DropdownMenuGroup>
            {orgs?.map((org) => (
              <DropdownMenuItem
                key={org.id}
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
            ))}
            <DropdownMenuItem className='cursor-pointer' onClick={() => navigate(ROUTES.ROOT)}>
              <Building2 className='h-4 w-4' />
              Manage organizations
            </DropdownMenuItem>
          </DropdownMenuGroup>

          {/* Logout */}
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
    </div>
  );
}
