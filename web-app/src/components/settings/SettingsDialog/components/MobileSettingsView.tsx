import type { LucideIcon } from "lucide-react";
import { ChevronLeft, ChevronRight, X } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { DialogClose } from "@/components/ui/shadcn/dialog";
import type { SettingsSection } from "@/stores/useSettingsDialog";
import type { Organization, OrgRole } from "@/types/organization";
import type { Workspace } from "@/types/workspace";
import { ActiveSection } from "./ActiveSection";
import { VersionBadge } from "./VersionBadge";

type NavIcon = LucideIcon | React.ComponentType<{ className?: string }>;

interface NavItem {
  value: SettingsSection;
  label: string;
  icon: NavIcon;
}

interface NavGroup {
  label: string;
  subtitle?: string;
  items: NavItem[];
}

interface MobileSettingsViewProps {
  visibleGroups: NavGroup[];
  activeSection: SettingsSection;
  activeLabel: string;
  detailOpen: boolean;
  onOpenSection: (section: SettingsSection) => void;
  onBackToList: () => void;
  org: Organization | null;
  role: OrgRole | null;
  isAdmin: boolean;
  workspace: Workspace | null;
  isLocalMode: boolean;
  close: () => void;
}

/**
 * Mobile master/detail layout for the settings dialog.
 *
 * In list mode, shows grouped section items (iOS Settings style). Tapping a
 * row drills into detail mode which renders the section's content with a
 * back-button header. The desktop dialog uses a side nav + content split
 * and lives directly in SettingsDialog/index.tsx.
 */
export function MobileSettingsView({
  visibleGroups,
  activeSection,
  activeLabel,
  detailOpen,
  onOpenSection,
  onBackToList,
  org,
  role,
  isAdmin,
  workspace,
  isLocalMode,
  close
}: MobileSettingsViewProps) {
  return (
    <div className='flex h-full min-h-0 min-w-0 flex-col md:hidden'>
      <MobileHeader
        mode={detailOpen ? "detail" : "list"}
        title={detailOpen ? activeLabel : "Settings"}
        onBack={onBackToList}
      />
      {detailOpen ? (
        <div className='min-h-0 flex-1 overflow-auto p-4'>
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
      ) : (
        <div className='min-h-0 flex-1 overflow-auto'>
          <div className='flex flex-col gap-6 p-4 pb-8'>
            {visibleGroups.map((group) => (
              <section key={group.label} className='flex flex-col gap-1.5'>
                <header className='px-1'>
                  <h2 className='font-semibold text-muted-foreground text-xs uppercase tracking-wider'>
                    {group.label}
                  </h2>
                  {group.subtitle && (
                    <p className='truncate text-muted-foreground/70 text-xs'>{group.subtitle}</p>
                  )}
                </header>
                <ul className='divide-y divide-border overflow-hidden rounded-xl border border-border bg-card'>
                  {group.items.map((item) => {
                    const Icon = item.icon;
                    return (
                      <li key={item.value}>
                        <button
                          type='button'
                          onClick={() => onOpenSection(item.value)}
                          className='flex w-full items-center gap-3 px-4 py-3.5 text-left transition-colors active:bg-muted'
                        >
                          <span className='flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-muted text-foreground'>
                            <Icon className='h-4 w-4' />
                          </span>
                          <span className='min-w-0 flex-1 truncate font-medium text-foreground text-sm'>
                            {item.label}
                          </span>
                          <ChevronRight className='h-4 w-4 shrink-0 text-muted-foreground' />
                        </button>
                      </li>
                    );
                  })}
                </ul>
              </section>
            ))}
            <div className='px-1 pt-2'>
              <VersionBadge />
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function MobileHeader({
  mode,
  title,
  onBack
}: {
  mode: "list" | "detail";
  title: string;
  onBack: () => void;
}) {
  return (
    <header className='flex h-14 shrink-0 items-center gap-1 border-border border-b bg-background px-2'>
      {mode === "detail" ? (
        <Button
          variant='ghost'
          size='icon'
          onClick={onBack}
          aria-label='Back to settings'
          className='h-10 w-10 shrink-0'
        >
          <ChevronLeft className='h-5 w-5' />
        </Button>
      ) : (
        <div className='h-10 w-10' aria-hidden='true' />
      )}
      <h1 className='min-w-0 flex-1 truncate text-center font-semibold text-base text-foreground'>
        {title}
      </h1>
      <DialogClose asChild>
        <Button
          variant='ghost'
          size='icon'
          aria-label='Close settings'
          className='h-10 w-10 shrink-0 text-muted-foreground hover:text-foreground'
        >
          <X className='h-5 w-5' />
        </Button>
      </DialogClose>
    </header>
  );
}
