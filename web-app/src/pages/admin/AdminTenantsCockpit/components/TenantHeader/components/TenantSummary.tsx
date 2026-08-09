import { FolderOpen, Users } from "lucide-react";
import type { AdminOrgMeta } from "@/services/api/adminTenants";

/**
 * At-a-glance state for the selected tenant, so the header answers "what am I looking
 * at" without scrolling into the dossier.
 *
 * Counts only. The partner relationship was here too until the rendered page showed it
 * sitting directly above the dossier's own "Managed by X" button — the same fact twice
 * in one viewport. No extra request either way: the rail has already loaded this, and a
 * header that fires its own fetch per selection makes clicking through tenants worse.
 */
export function TenantSummary({ org }: { org: AdminOrgMeta }) {
  return (
    <div className='flex items-center gap-3' data-testid='admin-tenant-summary'>
      <Stat
        icon={Users}
        value={org.member_count}
        label={org.member_count === 1 ? "member" : "members"}
      />
      <Stat
        icon={FolderOpen}
        value={org.workspace_count}
        label={org.workspace_count === 1 ? "workspace" : "workspaces"}
      />
    </div>
  );
}

function Stat({
  icon: Icon,
  value,
  label
}: {
  icon: React.ComponentType<{ className?: string }>;
  value: number;
  label: string;
}) {
  return (
    <span className='flex items-center gap-1 text-muted-foreground text-xs'>
      <Icon className='size-3' />
      <span className='text-foreground tabular-nums'>{value.toLocaleString()}</span>
      {label}
    </span>
  );
}
