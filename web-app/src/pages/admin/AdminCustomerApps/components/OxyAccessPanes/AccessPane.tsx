import { Factory, ShieldCheck, SlidersHorizontal } from "lucide-react";
import { useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import { useOxyAccessGrants } from "@/hooks/api/customerApps/useOxyAccessGrants";
import { cn } from "@/libs/shadcn/utils";
import ROUTES from "@/libs/utils/routes";
import { CommandSnippet } from "@/pages/admin/components/CommandSnippet";
import { CopyableId } from "@/pages/admin/components/CopyableId";
import type { OxyAccessGrant } from "@/types/apps";
import {
  EmptyHint,
  formatGrantedAt,
  GrantsError,
  GrantsLoading,
  groupByOrg,
  ListRow,
  MasterDetail,
  OpenHomeButton,
  type OrgGroup,
  PaneSearch
} from "./shared";

/**
 * Unified Oxy-access view — collapses the old "Orgs with access" and
 * "Projects with access" tabs (which rendered the same grant data grouped two
 * ways) into one surface. Left: the orgs that granted access. Right: that
 * org's workspaces, each showing its full grant metadata inline (no second
 * tab, no drill-to-see-IDs). Search matches org OR workspace name so you can
 * find a workspace and land on its org.
 */
export const AccessPane = () => {
  const { data, isLoading, error } = useOxyAccessGrants();
  const orgs = useMemo(() => groupByOrg(data ?? []), [data]);
  const [search, setSearch] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    if (!q) return orgs;
    return orgs.filter((o) => {
      if (`${o.orgName} ${o.orgSlug}`.toLowerCase().includes(q)) return true;
      return o.projects.some((p) => p.workspace_name.toLowerCase().includes(q));
    });
  }, [orgs, search]);

  const selected = orgs.find((o) => o.orgId === selectedId) ?? filtered[0] ?? null;
  const totalWorkspaces = useMemo(() => orgs.reduce((n, o) => n + o.projects.length, 0), [orgs]);

  if (error) return <GrantsError error={error} />;
  if (isLoading) return <GrantsLoading />;
  if (orgs.length === 0) {
    return (
      <EmptyHint
        title='No orgs have granted Oxy access'
        body='When an org owner enables Oxy access on a workspace, the org and its workspaces show up here.'
      />
    );
  }

  return (
    <MasterDetail
      list={
        <div className='flex h-full flex-col'>
          <PaneSearch
            value={search}
            onChange={setSearch}
            placeholder='Search orgs or workspaces…'
          />
          <div className='flex items-center justify-between px-3 py-1.5'>
            <span className='font-medium text-[10px] text-muted-foreground uppercase tracking-[0.14em]'>
              Orgs with access
            </span>
            <span className='font-mono text-[11px] text-muted-foreground tabular-nums'>
              {orgs.length} orgs · {totalWorkspaces} ws
            </span>
          </div>
          <div className='min-h-0 flex-1 overflow-auto border-border/60 border-t'>
            {filtered.map((o) => (
              <ListRow
                key={o.orgId}
                active={o.orgId === selected?.orgId}
                onClick={() => setSelectedId(o.orgId)}
                title={o.orgName}
                subtitle={o.orgSlug}
                trailing={
                  <span className='rounded bg-muted px-1.5 py-0.5 font-mono text-[11px] text-muted-foreground tabular-nums'>
                    {o.projects.length}
                  </span>
                }
              />
            ))}
          </div>
        </div>
      }
      detail={
        selected ? <OrgAccessDetail org={selected} highlight={search.trim().toLowerCase()} /> : null
      }
    />
  );
};

const OrgAccessDetail = ({ org, highlight }: { org: OrgGroup; highlight: string }) => (
  <div className='flex h-full flex-col overflow-auto'>
    <header className='flex items-center gap-3 border-border border-b px-5 py-4'>
      <div className='flex size-9 shrink-0 items-center justify-center rounded-md border bg-muted/40'>
        <ShieldCheck className='size-4 text-muted-foreground' />
      </div>
      <div className='min-w-0'>
        <h2 className='truncate font-semibold text-lg leading-tight'>{org.orgName}</h2>
        <p className='flex items-center gap-1 font-mono text-muted-foreground text-xs'>
          <span className='truncate'>{org.orgSlug}</span>
          <span aria-hidden>·</span>
          <CopyableId value={org.orgId} />
        </p>
      </div>
      <span className='ml-auto shrink-0 font-medium text-[11px] text-muted-foreground uppercase tracking-wide'>
        {org.projects.length} workspace{org.projects.length === 1 ? "" : "s"}
      </span>
    </header>
    <ul className='flex flex-col divide-y divide-border/50'>
      {org.projects.map((p) => (
        <WorkspaceAccessRow
          key={p.workspace_id}
          grant={p}
          dim={!!highlight && !p.workspace_name.toLowerCase().includes(highlight)}
        />
      ))}
    </ul>
  </div>
);

const WorkspaceAccessRow = ({ grant, dim }: { grant: OxyAccessGrant; dim?: boolean }) => (
  <li className={cn("flex items-start gap-4 px-5 py-3", dim && "opacity-50")}>
    <div className='min-w-0 flex-1 space-y-1.5'>
      <span className='block truncate font-medium text-sm'>{grant.workspace_name}</span>
      <dl className='grid grid-cols-[6rem_1fr] items-center gap-x-3 gap-y-1 text-xs'>
        <dt className='text-muted-foreground'>Workspace</dt>
        <dd className='min-w-0'>
          <CopyableId value={grant.workspace_id} />
        </dd>
        <dt className='text-muted-foreground'>Publish</dt>
        <dd className='min-w-0'>
          <CommandSnippet
            command={`oxy publish --env production --project ${grant.workspace_id}`}
          />
        </dd>
        <dt className='text-muted-foreground'>Granted</dt>
        <dd className='text-muted-foreground'>
          {formatGrantedAt(grant.granted_at)}
          {grant.granted_by_email ? ` · ${grant.granted_by_email}` : ""}
        </dd>
      </dl>
    </div>
    <div className='flex shrink-0 items-center gap-1.5'>
      {/* Jump straight into this workspace's Oxygen Factory (IDE). Opens in a
          new tab so the admin keeps their place in the access list rather
          than navigating the whole shell into another org's workspace. */}
      <Button asChild size='sm' variant='ghost' className='h-8 gap-1.5 text-muted-foreground'>
        <Link
          to={ROUTES.ORG(grant.org_slug).WORKSPACE(grant.workspace_id).IDE.ROOT}
          target='_blank'
          rel='noopener'
          title='Open in Oxy Factory (IDE)'
        >
          <Factory className='size-3.5' />
          Factory
        </Link>
      </Button>
      <Button asChild size='sm' variant='ghost' className='h-8 gap-1.5 text-muted-foreground'>
        <Link to={ROUTES.ADMIN.WORKSPACE_DETAIL(grant.workspace_id)} title='Manage workspace'>
          <SlidersHorizontal className='size-3.5' />
          Manage
        </Link>
      </Button>
      <OpenHomeButton grant={grant} />
    </div>
  </li>
);
