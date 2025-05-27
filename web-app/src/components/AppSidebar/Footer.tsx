import { LogOut, User2 } from "lucide-react";
import { useEffect, useState, useCallback } from "react";
import {
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/shadcn/sidebar";

interface UserInfo {
  email: string;
  picture?: string;
}

function useCurrentUserInfo() {
  const [userInfo, setUserInfo] = useState<UserInfo | null>(() => {
    const cached = sessionStorage.getItem("current_user_info");
    return cached ? JSON.parse(cached) : null;
  });

  const fetchUserInfo = useCallback(async () => {
    try {
      const res = await fetch("/api/user", { credentials: "include" });
      if (!res.ok) throw new Error();
      const data = await res.json();
      const info: UserInfo = {
        email: data && typeof data.email === "string" ? data.email : "unknown",
        picture:
          data && typeof data.picture === "string" ? data.picture : undefined,
      };
      setUserInfo(info);
      sessionStorage.setItem("current_user_info", JSON.stringify(info));
    } catch {
      const fallbackInfo: UserInfo = { email: "unknown" };
      setUserInfo(fallbackInfo);
      sessionStorage.setItem("current_user_info", JSON.stringify(fallbackInfo));
    }
  }, []);

  useEffect(() => {
    if (!userInfo) {
      fetchUserInfo();
    }
  }, [userInfo, fetchUserInfo]);

  return userInfo;
}

export function Footer() {
  const userInfo = useCurrentUserInfo();

  return (
    <div className="mt-auto px-2 pb-4">
      <SidebarMenu>
        {userInfo && (
          <SidebarMenuItem>
            <div
              className="flex items-center gap-3 w-full px-2 py-3 text-sm border-t pt-4"
              title={userInfo.email}
            >
              {userInfo.picture ? (
                <img
                  src={userInfo.picture}
                  alt={userInfo.email}
                  className="w-8 h-8 rounded-full object-cover"
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
                className={`w-8 h-8 text-muted-foreground ${userInfo.picture ? "hidden" : ""}`}
              />
              <span className="truncate text-muted-foreground">
                {userInfo.email}
              </span>
            </div>
          </SidebarMenuItem>
        )}
        <SidebarMenuItem>
          <SidebarMenuButton asChild>
            <a
              href={`${window.location.origin}?gcp-iap-mode=CLEAR_LOGIN_COOKIE`}
              className="flex items-center gap-2 w-full"
            >
              <LogOut className="w-4 h-4" />
              <span>Logout</span>
            </a>
          </SidebarMenuButton>
        </SidebarMenuItem>
      </SidebarMenu>
    </div>
  );
}
