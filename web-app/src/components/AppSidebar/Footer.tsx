import { ChevronsUpDown, LogOut, Settings } from "lucide-react";
import { useEffect, useState } from "react";
import { SidebarMenu, SidebarMenuItem } from "@/components/ui/shadcn/sidebar";
import { useAuth } from "@/contexts/AuthContext";
import useSettingsPage from "@/stores/useSettingsPage";
import { Avatar, AvatarFallback, AvatarImage } from "../ui/shadcn/avatar";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger
} from "../ui/shadcn/dropdown-menu";

interface IAPUserInfo {
  email: string;
  picture?: string;
}

export function Footer() {
  const [userIAPInfo, setUserIAPInfo] = useState<IAPUserInfo | null>(null);
  const { logout, getUser, authConfig } = useAuth();
  const { setIsOpen: setIsSettingsOpen } = useSettingsPage();

  useEffect(() => {
    (async () => {
      if (authConfig.is_built_in_mode) {
        return;
      }
      try {
        const res = await fetch("/api/user", { credentials: "include" });
        if (!res.ok) throw new Error();
        const data = await res.json();
        setUserIAPInfo({
          email: data?.email || "unknown",
          picture: data?.picture
        });
      } catch {
        setUserIAPInfo({ email: "unknown" });
      }
    })();
  }, [authConfig.is_built_in_mode]);

  const parsedUser = JSON.parse(getUser() || "null");

  let user = authConfig.is_built_in_mode ? parsedUser : userIAPInfo;

  if (!user) {
    user = {
      email: "guest@oxy.local",
      picture: undefined,
      isGuest: true
    };
  }

  return (
    <div className='mt-auto px-2 pb-4'>
      <SidebarMenu>
        <SidebarMenuItem>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <div
                className='flex w-full cursor-pointer items-center gap-3 rounded-md px-2 py-3 pt-4 text-sm transition-colors hover:bg-sidebar-accent hover:text-sidebar-accent-foreground'
                title={user.email || "User Options"}
              >
                <Avatar className='h-8 w-8 rounded-lg'>
                  <AvatarImage src={user.picture} alt={user.email} />
                  <AvatarFallback className='rounded-lg'>{user.email.charAt(0)}</AvatarFallback>
                </Avatar>
                <span className='truncate'>{user.email}</span>
                <ChevronsUpDown className='ml-auto size-4' />
              </div>
            </DropdownMenuTrigger>
            <DropdownMenuContent align='end' className='w-56'>
              {authConfig.cloud && (
                <DropdownMenuItem
                  className='cursor-pointer'
                  onClick={() => setIsSettingsOpen(true)}
                >
                  <Settings className='mr-2 h-4 w-4' />
                  <span>Settings</span>
                </DropdownMenuItem>
              )}

              {!user.isGuest && (
                <>
                  <DropdownMenuSeparator />
                  <DropdownMenuItem
                    className='cursor-pointer text-red-600 focus:text-red-600'
                    onClick={logout}
                  >
                    <LogOut className='mr-2 h-4 w-4' />
                    <span>Logout</span>
                  </DropdownMenuItem>
                </>
              )}
            </DropdownMenuContent>
          </DropdownMenu>
        </SidebarMenuItem>
      </SidebarMenu>
    </div>
  );
}
