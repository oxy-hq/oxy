import { VisuallyHidden } from "@radix-ui/react-visually-hidden";
import {
  Activity,
  CreditCard,
  Database,
  GitBranch,
  Key,
  KeyRound,
  type LucideIcon,
  Plug,
  Settings as SettingsIcon,
  Users
} from "lucide-react";
import { useEffect, useState } from "react";
import { AirhouseLogo } from "@/components/icons";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/shadcn/dialog";
import { useAuth } from "@/contexts/AuthContext";
import { FEATURES } from "@/libs/features";
import { cn } from "@/libs/shadcn/utils";
import useCurrentOrg from "@/stores/useCurrentOrg";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";
import useSettingsDialog, { type SettingsSection } from "@/stores/useSettingsDialog";
import { ActiveSection } from "./components/ActiveSection";
import { MobileSettingsView } from "./components/MobileSettingsView";
import { VersionBadge } from "./components/VersionBadge";

type NavIcon = LucideIcon | React.ComponentType<{ className?: string }>;

interface NavItem {
  value: SettingsSection;
  label: string;
  icon: NavIcon;
  adminOnly?: boolean;
  featureFlag?: keyof typeof FEATURES;
  /** When set, only render this item if the matching authConfig flag is true. */
  requiresBilling?: boolean;
}

interface NavGroup {
  label: string;
  items: NavItem[];
}

const CLOUD_NAV: NavGroup[] = [
  {
    label: "Organization",
    items: [
      { value: "organization.general", label: "General", icon: SettingsIcon },
      { value: "organization.members", label: "Members", icon: Users },
      {
        value: "organization.billing",
        label: "Billing",
        icon: CreditCard,
        adminOnly: true,
        requiresBilling: true
      },
      { value: "organization.integration", label: "Integration", icon: Plug }
    ]
  },
  {
    label: "Workspace",
    items: [
      { value: "workspace.members", label: "Members", icon: Users },
      { value: "workspace.databases", label: "Databases", icon: Database },
      { value: "workspace.airhouse", label: "Airhouse", icon: AirhouseLogo },
      {
        value: "workspace.repositories",
        label: "Repositories",
        icon: GitBranch,
        featureFlag: "LINKED_REPOS"
      },
      { value: "workspace.api_keys", label: "API Keys", icon: Key },
      { value: "workspace.secrets", label: "Secrets", icon: KeyRound, adminOnly: true },
      { value: "workspace.activity_logs", label: "Activity Logs", icon: Activity }
    ]
  }
];

const LOCAL_NAV: NavGroup[] = [
  {
    label: "Workspace",
    items: [
      { value: "workspace.databases", label: "Databases", icon: Database },
      { value: "workspace.airhouse", label: "Airhouse", icon: AirhouseLogo },
      { value: "workspace.api_keys", label: "API Keys", icon: Key },
      { value: "workspace.secrets", label: "Secrets", icon: KeyRound },
      { value: "workspace.activity_logs", label: "Activity Logs", icon: Activity }
    ]
  }
];

export default function SettingsDialog() {
  const { isOpen, section, open, close } = useSettingsDialog();
  const { isLocalMode, authConfig } = useAuth();
  const { org } = useCurrentOrg();
  const role = useCurrentOrg((s) => s.role);
  const { workspace } = useCurrentWorkspace();

  const isAdmin = role === "owner" || role === "admin";
  const billingEnabled = authConfig.billing_enabled;
  const [mobileDetailOpen, setMobileDetailOpen] = useState(false);

  // Always land on the section list when the dialog opens on mobile, so users
  // see all available config groups instead of being dropped into one section.
  useEffect(() => {
    if (!isOpen) setMobileDetailOpen(false);
  }, [isOpen]);

  const groupSubtitle = (groupLabel: string): string | undefined => {
    if (groupLabel === "Organization") return org?.name;
    if (groupLabel === "Workspace") return workspace?.name;
    return undefined;
  };

  const groupAvailable = (groupLabel: string): boolean => {
    if (groupLabel === "Organization") return !!org && !!role;
    if (groupLabel === "Workspace") return !!workspace;
    return true;
  };

  const navConfig = isLocalMode ? LOCAL_NAV : CLOUD_NAV;

  const visibleGroups = navConfig
    .filter((g) => groupAvailable(g.label))
    .map((g) => ({
      ...g,
      subtitle: groupSubtitle(g.label),
      items: g.items.filter(
        (i) =>
          (!i.featureFlag || FEATURES[i.featureFlag]) &&
          (!i.adminOnly || isLocalMode || isAdmin) &&
          (!i.requiresBilling || billingEnabled)
      )
    }))
    .filter((g) => g.items.length > 0);

  const allItems = visibleGroups.flatMap((g) => g.items);
  const activeSection = allItems.some((i) => i.value === section)
    ? section
    : (allItems[0]?.value ?? "organization.general");

  // Keep the store in sync with what's actually rendered so external readers
  // of `useSettingsDialog().section` don't see a value that's filtered out
  // (e.g. store default is "organization.general" but in local mode only
  // workspace sections are available).
  useEffect(() => {
    if (isOpen && section !== activeSection) {
      open(activeSection);
    }
  }, [isOpen, section, activeSection, open]);

  if (visibleGroups.length === 0) return null;

  const activeItem = allItems.find((i) => i.value === activeSection);
  const activeLabel = activeItem?.label ?? "Settings";

  return (
    <Dialog open={isOpen} onOpenChange={(v) => !v && close()}>
      <DialogContent
        className='top-0 left-0 h-[100svh] w-screen max-w-none translate-x-0 translate-y-0 gap-0 overflow-hidden rounded-none p-0 sm:top-1/2 sm:left-1/2 sm:h-[min(720px,90vh)] sm:max-w-5xl sm:-translate-x-1/2 sm:-translate-y-1/2 sm:rounded-lg'
        showCloseButton={false}
      >
        <VisuallyHidden>
          <DialogTitle>Settings</DialogTitle>
        </VisuallyHidden>

        <MobileSettingsView
          visibleGroups={visibleGroups}
          activeSection={activeSection}
          activeLabel={activeLabel}
          detailOpen={mobileDetailOpen}
          onOpenSection={(section) => {
            open(section);
            setMobileDetailOpen(true);
          }}
          onBackToList={() => setMobileDetailOpen(false)}
          org={org}
          role={role}
          isAdmin={isAdmin}
          workspace={workspace}
          isLocalMode={isLocalMode}
          close={close}
        />

        {/* Desktop layout — side nav + content */}
        <div className='hidden h-full min-h-0 min-w-0 md:flex'>
          <nav className='flex w-60 shrink-0 flex-col gap-4 overflow-y-auto border-sidebar-border border-r bg-sidebar p-3'>
            {visibleGroups.map((group) => (
              <div key={group.label} className='flex flex-col gap-1'>
                <div className='px-2.5 pt-1 pb-0.5'>
                  <p className='font-semibold text-muted-foreground text-xs uppercase tracking-wider'>
                    {group.label}
                  </p>
                  {group.subtitle && (
                    <p className='truncate text-muted-foreground/70 text-xs'>{group.subtitle}</p>
                  )}
                </div>
                <ul className='flex flex-col gap-0.5'>
                  {group.items.map((item) => {
                    const Icon = item.icon;
                    const isActive = item.value === activeSection;
                    return (
                      <li key={item.value}>
                        <button
                          type='button'
                          onClick={() => open(item.value)}
                          data-active={isActive}
                          className={cn(
                            "flex h-8 w-full items-center gap-2.5 rounded-md px-2.5 text-left font-medium text-sidebar-foreground text-sm outline-none transition-colors",
                            "hover:bg-sidebar-accent hover:text-sidebar-accent-foreground",
                            "data-[active=true]:bg-sidebar-accent data-[active=true]:text-sidebar-accent-foreground"
                          )}
                        >
                          <Icon className='h-4 w-4 shrink-0' />
                          <span className='truncate'>{item.label}</span>
                        </button>
                      </li>
                    );
                  })}
                </ul>
              </div>
            ))}

            <div className='mt-auto px-2 pt-2'>
              <VersionBadge />
            </div>
          </nav>

          <div className='min-h-0 min-w-0 flex-1 overflow-auto p-6'>
            <ActiveSection
              activeSection={activeSection}
              org={org}
              role={role}
              isAdmin={isAdmin}
              workspace={workspace}
              isLocalMode={isLocalMode}
              close={close}
            />
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
