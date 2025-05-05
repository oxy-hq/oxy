import { LogOut, User2 } from "lucide-react";
import { useEffect, useState, useCallback } from "react";
import {
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/shadcn/sidebar";

function useCurrentUserEmail() {
  const [email, setEmail] = useState<string | null>(() =>
    sessionStorage.getItem("current_user_email"),
  );

  const fetchEmail = useCallback(async () => {
    try {
      const res = await fetch("/oauth2/userinfo", { credentials: "include" });
      if (!res.ok) throw new Error();
      const data = await res.json();
      const userEmail =
        data && typeof data.email === "string" ? data.email : "unknown";
      setEmail(userEmail);
      sessionStorage.setItem("current_user_email", userEmail);
    } catch {
      setEmail("unknown");
      sessionStorage.setItem("current_user_email", "unknown");
    }
  }, []);

  useEffect(() => {
    if (!email) {
      fetchEmail();
    }
  }, [email, fetchEmail]);

  return email;
}

export function Footer() {
  const currentUserEmail = useCurrentUserEmail();

  return (
    <div className="mt-auto px-2 pb-4">
      <SidebarMenu>
        {currentUserEmail && (
          <SidebarMenuItem>
            <div
              className="flex items-center gap-2 w-full px-2 py-2 text-xs text-muted-foreground truncate"
              title={currentUserEmail}
            >
              <User2 className="w-4 h-4" />
              <span className="truncate">{currentUserEmail}</span>
            </div>
          </SidebarMenuItem>
        )}
        <SidebarMenuItem>
          <SidebarMenuButton asChild>
            <a
              href={`${window.location.origin}/oauth2/sign_out`}
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
