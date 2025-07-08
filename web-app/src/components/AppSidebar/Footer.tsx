import {
  User2,
  LogOut,
  Key,
  Settings,
  Database,
  Shield,
  Users,
} from "lucide-react";
import { useEffect, useState } from "react";
import { SidebarMenu, SidebarMenuItem } from "@/components/ui/shadcn/sidebar";
import { useNavigate } from "react-router-dom";
import { useAuth } from "@/contexts/AuthContext";
import { useReadonly } from "@/hooks/useReadonly";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "../ui/shadcn/dropdown-menu";

interface IAPUserInfo {
  email: string;
  picture?: string;
}

export function Footer() {
  const [userIAPInfo, setUserIAPInfo] = useState<IAPUserInfo | null>(null);
  const navigate = useNavigate();
  const { logout, getUser, authConfig } = useAuth();
  const { isReadonly } = useReadonly();

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
                  className="flex items-center gap-3 w-full px-2 py-3 text-sm border-t pt-4 cursor-pointer hover:bg-sidebar-accent hover:text-sidebar-accent-foreground rounded-md transition-colors"
                  title={user?.email || "User Options"}
                >
                  {user?.picture ? (
                    <img
                      src={user.picture}
                      alt={user.email}
                      className="w-4 h-4 rounded-full object-cover"
                      onError={(e) => {
                        // Fallback to icon if image fails to load
                        e.currentTarget.style.display = "none";
                        e.currentTarget.nextElementSibling?.classList.remove(
                          "hidden",
                        );
                      }}
                    />
                  ) : null}
                  <User2
                    className={`w-4 h-4 text-muted-foreground ${user?.picture ? "hidden" : ""}`}
                  />
                  <span className="truncate text-muted-foreground">
                    {user?.email || "Unknown user"}
                  </span>
                </div>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-56">
                {isReadonly && (
                  <DropdownMenuItem
                    className="cursor-pointer"
                    onClick={() => navigate("/github-settings")}
                  >
                    <Settings className="w-4 h-4 mr-2" />
                    <span>Github Settings</span>
                  </DropdownMenuItem>
                )}
                <DropdownMenuItem
                  className="cursor-pointer"
                  onClick={() => navigate("/secrets")}
                >
                  <Shield className="w-4 h-4 mr-2" />
                  <span>Secret Management</span>
                </DropdownMenuItem>
                <DropdownMenuItem
                  className="cursor-pointer"
                  onClick={() => navigate("/databases")}
                >
                  <Database className="w-4 h-4 mr-2" />
                  <span>Databases</span>
                </DropdownMenuItem>
                <DropdownMenuItem
                  className="cursor-pointer"
                  onClick={() => navigate("/users")}
                >
                  <Users className="w-4 h-4 mr-2" />
                  <span>Users</span>
                </DropdownMenuItem>
                <DropdownMenuItem
                  className="cursor-pointer"
                  onClick={() => navigate("/api-keys")}
                >
                  <Key className="w-4 h-4 mr-2" />
                  <span>API Keys</span>
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
