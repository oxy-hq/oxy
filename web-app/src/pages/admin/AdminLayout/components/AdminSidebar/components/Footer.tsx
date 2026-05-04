import { LogOut } from "lucide-react";
import { UserAvatar } from "@/components/UserAvatar";
import { Button } from "@/components/ui/shadcn/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger
} from "@/components/ui/shadcn/dropdown-menu";
import { useAuth } from "@/contexts/AuthContext";
import type { UserInfo } from "@/types/auth";

function useLocalUserInfo(): UserInfo | null {
  const { getUser } = useAuth();
  try {
    return JSON.parse(getUser() || "null");
  } catch {
    return null;
  }
}

export function Footer() {
  const { logout } = useAuth();
  const currentUser = useLocalUserInfo();

  const email = currentUser?.email ?? "guest@oxy.local";
  const name = currentUser?.name ?? "";
  const picture = currentUser?.picture;
  const isGuest = !currentUser;

  return (
    <div className='border-sidebar-border/50 border-t p-2'>
      <DropdownMenu>
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

        {!isGuest && (
          <DropdownMenuContent side='top' align='start' sideOffset={4} className='min-w-56'>
            <DropdownMenuItem
              className='cursor-pointer text-destructive focus:text-destructive'
              onClick={logout}
            >
              <LogOut className='h-4 w-4' />
              <span>Log out</span>
            </DropdownMenuItem>
          </DropdownMenuContent>
        )}
      </DropdownMenu>
    </div>
  );
}
