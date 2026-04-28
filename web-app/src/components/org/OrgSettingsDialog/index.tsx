import { VisuallyHidden } from "@radix-ui/react-visually-hidden";
import type { LucideIcon } from "lucide-react";
import { Plug, Settings as SettingsIcon, Users } from "lucide-react";
import { useEffect, useState } from "react";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/shadcn/dialog";
import { cn } from "@/libs/shadcn/utils";
import type { Organization, OrgRole } from "@/types/organization";
import GeneralSection from "./GeneralSection";
import IntegrationSection from "./IntegrationSection";
import TeamSection from "./TeamSection";

export type OrgSettingsTab = "general" | "team" | "integration";

interface OrgSettingsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  org: Organization;
  viewerRole: OrgRole;
  defaultTab?: OrgSettingsTab;
}

const NAV_ITEMS: { value: OrgSettingsTab; label: string; icon: LucideIcon }[] = [
  { value: "general", label: "General", icon: SettingsIcon },
  { value: "team", label: "Team", icon: Users },
  { value: "integration", label: "Integrations", icon: Plug }
];

export default function OrgSettingsDialog({
  open,
  onOpenChange,
  org,
  viewerRole,
  defaultTab = "general"
}: OrgSettingsDialogProps) {
  const [tab, setTab] = useState<OrgSettingsTab>(defaultTab);

  useEffect(() => {
    if (open) {
      setTab(defaultTab);
    }
  }, [open, defaultTab]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='max-w-3xl overflow-hidden p-0 sm:max-w-3xl' showCloseButton={false}>
        <VisuallyHidden>
          <DialogTitle>Organization settings</DialogTitle>
        </VisuallyHidden>
        <div className='flex h-[min(620px,100vh)]'>
          <nav className='flex w-56 shrink-0 flex-col gap-4 border-sidebar-border border-r bg-sidebar p-3'>
            <div className='min-w-0 px-2 pt-1'>
              <p className='font-semibold text-[13px] text-sidebar-foreground'>Settings</p>
              <p className='truncate text-[11px] text-muted-foreground'>{org.name}</p>
            </div>
            <ul className='flex flex-col gap-1'>
              {NAV_ITEMS.map((item) => {
                const Icon = item.icon;
                const isActive = item.value === tab;
                return (
                  <li key={item.value}>
                    <button
                      type='button'
                      onClick={() => setTab(item.value)}
                      data-active={isActive}
                      className={cn(
                        "flex h-8 w-full items-center gap-2.5 rounded-md px-2.5 text-left font-medium text-[13px] text-sidebar-foreground outline-none transition-colors",
                        "hover:bg-sidebar-accent hover:text-sidebar-accent-foreground",
                        "data-[active=true]:bg-sidebar-accent data-[active=true]:text-sidebar-accent-foreground"
                      )}
                    >
                      <Icon className='h-[15px] w-[15px] shrink-0' />
                      <span>{item.label}</span>
                    </button>
                  </li>
                );
              })}
            </ul>
          </nav>

          <div className='flex-1 overflow-auto p-6'>
            {tab === "general" && <GeneralSection org={org} onClose={() => onOpenChange(false)} />}
            {tab === "team" && <TeamSection org={org} viewerRole={viewerRole} />}
            {tab === "integration" && <IntegrationSection org={org} />}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
