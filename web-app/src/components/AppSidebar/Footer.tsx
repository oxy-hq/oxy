import { LogOut, User2 } from "lucide-react";
import { useEffect, useState } from "react";
import Cookies from "js-cookie";
import {
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/shadcn/sidebar";

interface UserInfo {
  email: string;
  picture?: string;
}

const clearAllCookies = () => {
  const cookies = document.cookie.split(";");
  for (const cookie of cookies) {
    const eqPos = cookie.indexOf("=");
    const name = eqPos > -1 ? cookie.slice(0, eqPos).trim() : cookie.trim();

    if (name) {
      Cookies.remove(name);
      Cookies.remove(name, { path: "/" });
      Cookies.remove(name, { path: "/", domain: window.location.hostname });
      const domain = window.location.hostname.split(".").slice(-2).join(".");
      Cookies.remove(name, { path: "/", domain: `.${domain}` });
    }
  }
};

const handleLogout = async () => {
  localStorage.clear();
  sessionStorage.clear();
  clearAllCookies();

  // Redirect to home page
  window.location.href = window.location.origin;
};

export function Footer() {
  const [userInfo, setUserInfo] = useState<UserInfo | null>(null);

  useEffect(() => {
    (async () => {
      try {
        const res = await fetch("/api/user", { credentials: "include" });
        if (!res.ok) throw new Error();
        const data = await res.json();
        setUserInfo({
          email: data?.email || "unknown",
          picture: data?.picture,
        });
      } catch {
        setUserInfo({ email: "unknown" });
      }
    })();
  }, []);

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
                className={`w-4 h-4 text-muted-foreground ${userInfo.picture ? "hidden" : ""}`}
              />
              <span className="truncate text-muted-foreground">
                {userInfo.email}
              </span>
            </div>
          </SidebarMenuItem>
        )}
        <SidebarMenuItem>
          <SidebarMenuButton asChild>
            <button
              onClick={handleLogout}
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
