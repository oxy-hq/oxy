import { Boxes, Factory, Home, ShieldCheck, ShieldOff, SlidersHorizontal } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { cn } from "@/libs/shadcn/utils";
import ROUTES from "@/libs/utils/routes";
import { CopyableId } from "@/pages/admin/components/CopyableId";
import type { OxyAccessGrant } from "@/types/apps";
import { openWorkspaceHome } from "../../openWorkspaceHome";
import type { AccessOrg } from "./accessModel";
import { CopyPublishAction, formatGrantedAt, RowAction } from "./shared";

/**
 * Right pane of the Access cockpit. For an org that granted access: its
 * workspaces with grant metadata inline + icon actions. For an org without
 * access: a compact empty state explaining it hasn't opted in — so "no access"
 * is a first-class, inspectable state, not just an absence from the list.
 */
export const OrgAccessDetail = ({ org, highlight }: { org: AccessOrg; highlight: string }) => (
  <div className='flex h-full flex-col overflow-auto'>
    <header className='sticky top-0 z-10 flex items-center gap-2.5 border-b bg-background px-4 py-2.5'>
      <div className='flex size-7 shrink-0 items-center justify-center rounded-md border bg-muted/40'>
        {org.hasAccess ? (
          <ShieldCheck className='size-3.5 text-muted-foreground' />
        ) : (
          <ShieldOff className='size-3.5 text-muted-foreground/60' />
        )}
      </div>
      <div className='min-w-0'>
        <h2 className='truncate font-semibold text-sm leading-tight'>{org.orgName}</h2>
        <p className='flex items-center gap-1 font-mono text-[11px] text-muted-foreground'>
          <span className='truncate'>{org.orgSlug}</span>
          <span aria-hidden>·</span>
          <CopyableId value={org.orgId} />
        </p>
      </div>
      <span className='ml-auto shrink-0 font-mono text-[11px] text-muted-foreground tabular-nums'>
        {org.hasAccess ? `${org.grants.length} granted` : `${org.workspaceCount} ws`}
      </span>
    </header>
    {org.hasAccess ? (
      <ul className='flex flex-col divide-y divide-border/50'>
        {org.grants.map((g) => (
          <WorkspaceAccessRow
            key={g.workspace_id}
            grant={g}
            dim={!!highlight && !g.workspace_name.toLowerCase().includes(highlight)}
          />
        ))}
      </ul>
    ) : (
      <NoAccessBody workspaceCount={org.workspaceCount} />
    )}
  </div>
);

const NoAccessBody = ({ workspaceCount }: { workspaceCount: number }) => (
  <div className='flex flex-1 flex-col items-center justify-center gap-2 p-8 text-center'>
    <div className='flex size-10 items-center justify-center rounded-full border bg-muted/30'>
      <ShieldOff className='size-4 text-muted-foreground' />
    </div>
    <p className='font-medium text-sm'>No Oxy access</p>
    <p className='max-w-xs text-muted-foreground text-sm'>
      This org has not granted Oxy access to any of its {workspaceCount} workspace
      {workspaceCount === 1 ? "" : "s"}. An org owner enables it per workspace.
    </p>
  </div>
);

const WorkspaceAccessRow = ({ grant, dim }: { grant: OxyAccessGrant; dim?: boolean }) => {
  const navigate = useNavigate();
  return (
    <li className={cn("flex items-center gap-2.5 px-4 py-2", dim && "opacity-40")}>
      <Boxes className='size-4 shrink-0 text-muted-foreground/60' />
      <div className='min-w-0 flex-1'>
        <div className='flex items-center gap-2'>
          <span className='truncate font-medium text-sm'>{grant.workspace_name}</span>
          <CopyableId value={grant.workspace_id} className='shrink-0' />
        </div>
        <p className='truncate font-mono text-[11px] text-muted-foreground'>
          granted {formatGrantedAt(grant.granted_at)}
          {grant.granted_by_email ? ` · ${grant.granted_by_email}` : ""}
        </p>
      </div>
      <div className='flex shrink-0 items-center gap-0.5'>
        <CopyPublishAction workspaceId={grant.workspace_id} />
        {/* New tab so the admin keeps their place in the access list. */}
        <RowAction
          icon={Factory}
          label='Open in Oxy Factory (IDE)'
          onClick={() =>
            window.open(
              ROUTES.ORG(grant.org_slug).WORKSPACE(grant.workspace_id).IDE.ROOT,
              "_blank",
              "noopener"
            )
          }
        />
        <RowAction
          icon={SlidersHorizontal}
          label='Manage workspace'
          onClick={() => navigate(ROUTES.ADMIN.WORKSPACE_DETAIL(grant.workspace_id))}
        />
        <RowAction
          icon={Home}
          label='Open /home (new tab)'
          onClick={() => openWorkspaceHome(grant)}
        />
      </div>
    </li>
  );
};
