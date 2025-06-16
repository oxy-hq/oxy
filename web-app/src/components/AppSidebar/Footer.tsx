import { LogOut, User2 } from "lucide-react";
import { useEffect, useState } from "react";
import {
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/shadcn/sidebar";
import { useNavigate } from "react-router-dom";
import { useAuth } from "@/contexts/AuthContext";
import { handleLogout } from "@/libs/utils";

interface UserInfo {
  email: string;
  picture?: string;
}

export function Footer() {
  const [userIAPInfo, setUserIAPInfo] = useState<UserInfo | null>(null);
  const navigate = useNavigate();
  const { logout, getUser, authConfig } = useAuth();

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

  const currentUser = authConfig.is_built_in_mode ? parsedUser : userIAPInfo;

  return (
    <div className="mt-auto px-2 pb-4">
      <SidebarMenu>
        {currentUser && (
          <SidebarMenuItem>
            <div
              className="flex items-center gap-3 w-full px-2 py-3 text-sm border-t pt-4"
              title={currentUser.email}
            >
              {currentUser.picture ? (
                <img
                  src={currentUser.picture}
                  alt={currentUser.email}
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
                className={`w-4 h-4 text-muted-foreground ${currentUser.picture ? "hidden" : ""}`}
              />
              <span className="truncate text-muted-foreground">
                {currentUser?.email}
              </span>
            </div>
          </SidebarMenuItem>
        )}
        <SidebarMenuItem>
          <SidebarMenuButton asChild>
            <button
              onClick={() => {
                if (!authConfig.is_built_in_mode) {
                  handleLogout();
                  return;
                }
                logout();
                navigate("/login");
              }}
              className="flex items-center gap-2 w-full"
            >
              <LogOut className="w-4 h-4" />
              <span>Logout</span>
            </button>
          </SidebarMenuButton>
        </SidebarMenuItem>
      </SidebarMenu>
    </div>
  );
}
