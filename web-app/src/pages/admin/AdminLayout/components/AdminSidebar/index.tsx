import { Flag, Inbox } from "lucide-react";
import { Link, useLocation } from "react-router-dom";
import OxyLogo from "@/components/OxyLogo";
import {
  Sidebar as ShadcnSidebar,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem
} from "@/components/ui/shadcn/sidebar";
import { Footer } from "./components/Footer";

const ADMIN_BILLING_QUEUE_ROUTE = "/admin/billing/queue";
const ADMIN_FEATURE_FLAGS_ROUTE = "/admin/feature-flags";

export function AdminSidebar() {
  const location = useLocation();
  const isBillingQueueActive = location.pathname.startsWith(ADMIN_BILLING_QUEUE_ROUTE);
  const isFeatureFlagsActive = location.pathname.startsWith(ADMIN_FEATURE_FLAGS_ROUTE);

  return (
    <ShadcnSidebar className='border-sidebar-border border-r bg-sidebar-background'>
      <div className='flex h-[52px] shrink-0 items-center gap-2 border-sidebar-border/50 border-b px-3'>
        <Link to={ADMIN_BILLING_QUEUE_ROUTE} className='flex shrink-0 items-center'>
          <OxyLogo />
        </Link>
        <span className='rounded bg-muted px-1.5 py-0.5 font-medium text-[10px] text-muted-foreground uppercase tracking-wider'>
          Admin
        </span>
      </div>

      <div className='min-h-0 flex-1 overflow-auto'>
        <SidebarGroup className='px-2 pt-2'>
          <SidebarGroupLabel>Operations</SidebarGroupLabel>
          <SidebarMenu>
            <SidebarMenuItem>
              <SidebarMenuButton asChild isActive={isBillingQueueActive}>
                <Link to={ADMIN_BILLING_QUEUE_ROUTE}>
                  <Inbox />
                  <span>Billing queue</span>
                </Link>
              </SidebarMenuButton>
            </SidebarMenuItem>
            <SidebarMenuItem>
              <SidebarMenuButton asChild isActive={isFeatureFlagsActive}>
                <Link to={ADMIN_FEATURE_FLAGS_ROUTE}>
                  <Flag />
                  <span>Feature flags</span>
                </Link>
              </SidebarMenuButton>
            </SidebarMenuItem>
          </SidebarMenu>
        </SidebarGroup>
      </div>

      <Footer />
    </ShadcnSidebar>
  );
}
