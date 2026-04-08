import { LogOut, Users } from "lucide-react";
import { Link } from "react-router-dom";
import { useAuth } from "@/contexts/AuthContext";
import ROUTES from "@/libs/utils/routes";
import type { UserInfo } from "@/types/auth";
import { UserAvatar } from "../UserAvatar";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger
} from "../ui/shadcn/dropdown-menu";

function useCurrentUser(): { user: UserInfo | null; isAdmin: boolean; isLocal: boolean } {
  const { getUser, authConfig } = useAuth();
  const isLocal = !authConfig.auth_enabled || !!authConfig.single_workspace;
  try {
    const user: UserInfo | null = JSON.parse(getUser() || "null");
    const isAdmin = isLocal || user?.is_admin === true;
    return { user, isAdmin, isLocal };
  } catch {
    return { user: null, isAdmin: isLocal, isLocal };
  }
}

function UserRow({
  name,
  email,
  picture,
  interactive
}: {
  name: string;
  email: string;
  picture?: string | null;
  interactive: boolean;
}) {
  return (
    <div
      className={`flex w-full items-center gap-2.5 rounded-md px-2 py-2 text-left ${interactive ? "cursor-pointer transition-colors hover:bg-sidebar-accent" : ""}`}
      title={email}
    >
      <UserAvatar
        name={name}
        email={email}
        picture={picture}
        className='h-8 w-8 shrink-0 rounded-lg'
      />
      <div className='grid min-w-0 flex-1 text-left leading-tight'>
        <span className='truncate font-medium text-[13px] text-sidebar-foreground'>
          {name || email.split("@")[0]}
        </span>
        <span className='truncate text-[11px] text-sidebar-foreground/50'>{email}</span>
      </div>
    </div>
  );
}

export function Footer() {
  const { logout } = useAuth();
  const { user: currentUser, isAdmin, isLocal } = useCurrentUser();

  const email = currentUser?.email ?? "guest@oxy.local";
  const name = currentUser?.name ?? "";
  const picture = currentUser?.picture;
  const isGuest = !currentUser;

  const showManageMembers = isAdmin && !isLocal;
  const showLogout = !isGuest;
  const hasActions = showManageMembers || showLogout;

  return (
    <div className='border-sidebar-border/50 border-t p-2'>
      {hasActions ? (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button type='button' className='w-full'>
              <UserRow name={name} email={email} picture={picture} interactive={true} />
            </button>
          </DropdownMenuTrigger>

          <DropdownMenuContent
            align='end'
            sideOffset={4}
            className='w-[var(--radix-dropdown-menu-trigger-width)] min-w-56 rounded-lg'
          >
            {showManageMembers && (
              <DropdownMenuGroup>
                <DropdownMenuItem asChild className='cursor-pointer'>
                  <Link to={ROUTES.MEMBERS}>
                    <Users />
                    <span>Manage members</span>
                  </Link>
                </DropdownMenuItem>
              </DropdownMenuGroup>
            )}

            {showManageMembers && showLogout && <DropdownMenuSeparator />}

            {showLogout && (
              <DropdownMenuItem
                className='cursor-pointer text-destructive focus:text-destructive'
                onClick={logout}
              >
                <LogOut />
                <span>Log out</span>
              </DropdownMenuItem>
            )}
          </DropdownMenuContent>
        </DropdownMenu>
      ) : (
        <UserRow name={name} email={email} picture={picture} interactive={false} />
      )}
    </div>
  );
}
