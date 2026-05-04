import { Outlet, useLocation } from "react-router-dom";
import { SidebarInset, SidebarProvider } from "@/components/ui/shadcn/sidebar";
import { AdminSidebar } from "./components/AdminSidebar";
import { AdminTopbar } from "./components/AdminTopbar";

const PAGE_TITLES: Record<string, string> = {
  "/admin/billing/queue": "Billing queue",
  "/admin/feature-flags": "Feature flags"
};

export default function AdminLayout() {
  const location = useLocation();
  const title = PAGE_TITLES[location.pathname] ?? "Admin";

  return (
    <SidebarProvider>
      <AdminSidebar />
      <SidebarInset>
        <AdminTopbar title={title} />
        <div className='min-h-0 flex-1 overflow-auto'>
          <Outlet />
        </div>
      </SidebarInset>
    </SidebarProvider>
  );
}
