import { Sparkles } from "lucide-react";
import {
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarMenuSubButton,
  SidebarMenuSubItem
} from "@/components/ui/shadcn/sidebar";
import { useCustomApps } from "@/hooks/api/customApps/useCustomApps";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

/**
 * Bespoke JavaScript apps Oxy engineers have published for this
 * workspace. Renders with the same SidebarMenuItem / Sub pattern as
 * Threads / Procedures / Apps so the spacing, indentation, and hover
 * affordances stay uniform — just a different icon + label.
 *
 * Sets itself apart from `.app.yml` Data Apps through:
 *   1. position (rendered after Apps, with a small top margin for
 *      breathing room — not a separate group)
 *   2. the Sparkles icon + the "Custom" qualifier
 *   3. clicking navigates same-tab to the canonical customer-apps URL
 *      (the app owns its own chrome — no IDE link)
 *
 * Hidden entirely when the workspace has no published apps so
 * workspaces that never use this feature carry zero stub UI.
 */
export function CustomApps() {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  const { data: apps, isPending } = useCustomApps(projectId);

  if (isPending || !apps || apps.length === 0) {
    return null;
  }

  return (
    <SidebarMenuItem className='mt-2'>
      <SidebarMenuButton asChild>
        <div>
          <Sparkles />
          <span>Custom Apps</span>
        </div>
      </SidebarMenuButton>
      <SidebarMenuSub className='ml-[15px]'>
        {apps.map((app) => (
          <SidebarMenuSubItem key={app.id}>
            <SidebarMenuSubButton asChild>
              <a href={app.url} data-testid={`custom-app-link-${app.slug}`}>
                <span>{app.name}</span>
              </a>
            </SidebarMenuSubButton>
          </SidebarMenuSubItem>
        ))}
      </SidebarMenuSub>
    </SidebarMenuItem>
  );
}
