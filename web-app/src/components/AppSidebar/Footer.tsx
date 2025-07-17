import { LogOut, Settings, ChevronsUpDown } from "lucide-react";
import { useEffect, useState } from "react";
import { SidebarMenu, SidebarMenuItem } from "@/components/ui/shadcn/sidebar";
import { useAuth } from "@/contexts/AuthContext";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "../ui/shadcn/dropdown-menu";
import { Avatar, AvatarFallback, AvatarImage } from "../ui/shadcn/avatar";
import useSettingsPage from "@/stores/useSettingsPage";

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
          picture: data?.picture,
        });
      } catch {
        setUserIAPInfo({ email: "unknown" });
      }
    })();
  }, [authConfig.is_built_in_mode]);

  const parsedUser = JSON.parse(getUser() || "null");

  const user = authConfig.is_built_in_mode ? parsedUser : userIAPInfo;

  // Show footer options if user exists OR if auth is not enabled
  const shouldShowFooter = user || !authConfig.auth_enabled;

  return (
    <div className="mt-auto px-2 pb-4">
      <SidebarMenu>
        {shouldShowFooter && (
          <SidebarMenuItem>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <div
                  className="flex items-center gap-3 w-full px-2 py-3 text-sm pt-4 cursor-pointer hover:bg-sidebar-accent hover:text-sidebar-accent-foreground rounded-md transition-colors"
                  title={user?.email || "User Options"}
                >
                  <Avatar className="h-8 w-8 rounded-lg">
                    <AvatarImage src={user.picture} alt={user.email} />
                    <AvatarFallback className="rounded-lg">
                      {user.email.charAt(0)}
                    </AvatarFallback>
                  </Avatar>
                  <span className="truncate">
                    {user?.email || "Unknown user"}
                  </span>
                  <ChevronsUpDown className="ml-auto size-4" />
                </div>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-56">
                <DropdownMenuItem
                  className="cursor-pointer"
                  onClick={() => setIsSettingsOpen(true)}
                >
                  <Settings className="w-4 h-4 mr-2" />
                  <span>Settings</span>
                </DropdownMenuItem>
                {user && (
                  <>
                    <DropdownMenuSeparator />
                    <DropdownMenuItem
                      className="cursor-pointer text-red-600 focus:text-red-600"
                      onClick={logout}
                    >
                      <LogOut className="w-4 h-4 mr-2" />
                      <span>Logout</span>
                    </DropdownMenuItem>
                  </>
                )}
              </DropdownMenuContent>
            </DropdownMenu>
          </SidebarMenuItem>
        )}
      </SidebarMenu>
    </div>
  );
}
