import { Github, SlidersHorizontal } from "lucide-react";
import { useState } from "react";
import { SidebarMenu, SidebarMenuButton, SidebarMenuItem } from "@/components/ui/shadcn/sidebar";
import useCurrentOrg from "@/stores/useCurrentOrg";
import GeneralTab from "./GeneralTab";
import GitHubTab from "./GitHubTab";

const SETTINGS_NAV = [
  { key: "general", label: "General", icon: SlidersHorizontal },
  { key: "github", label: "GitHub", icon: Github }
];

export default function OrgSettingsPage() {
  const org = useCurrentOrg((s) => s.org);
  const [activeTab, setActiveTab] = useState("general");

  if (!org) {
    return (
      <div className='flex min-h-screen items-center justify-center'>
        <div className='text-muted-foreground'>Organization not found.</div>
      </div>
    );
  }

  return (
    <div className='flex h-full'>
      <nav className='w-56 shrink-0 border-sidebar-border border-r bg-sidebar-background px-2 py-4'>
        <div className='mb-2 px-2'>
          <span className='font-semibold text-[13px] text-sidebar-foreground'>Settings</span>
        </div>

        <SidebarMenu>
          {SETTINGS_NAV.map((item) => (
            <SidebarMenuItem key={item.key}>
              <SidebarMenuButton
                isActive={activeTab === item.key}
                onClick={() => setActiveTab(item.key)}
                className='h-8 gap-2.5 rounded-md px-2.5 font-medium text-[13px]'
              >
                <item.icon className='shrink-0' />
                <span>{item.label}</span>
              </SidebarMenuButton>
            </SidebarMenuItem>
          ))}
        </SidebarMenu>
      </nav>

      <main className='min-w-0 flex-1 overflow-y-auto px-12 py-6'>
        <div className='mx-auto max-w-3xl'>
          {activeTab === "general" && <GeneralTab org={org} />}
          {activeTab === "github" && <GitHubTab org={org} />}
        </div>
      </main>
    </div>
  );
}
