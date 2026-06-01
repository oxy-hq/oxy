import { useMemo, useState } from "react";
import { useOxyAccessGrants } from "@/hooks/api/customerApps/useOxyAccessGrants";
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
 * Orgs tab: master-detail of organizations that granted Oxy access. Left
 * pane lists orgs (searchable); right pane shows the selected org's granted
 * projects, each with an "Open /home" jump.
 */
export const OrgsPane = () => {
  const { data, isLoading, error } = useOxyAccessGrants();
  const orgs = useMemo(() => groupByOrg(data ?? []), [data]);
  const [search, setSearch] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    if (!q) return orgs;
    return orgs.filter((o) => `${o.orgName} ${o.orgSlug}`.toLowerCase().includes(q));
  }, [orgs, search]);

  const selected = orgs.find((o) => o.orgId === selectedId) ?? filtered[0] ?? null;

  if (error) return <GrantsError error={error} />;
  if (isLoading) return <GrantsLoading />;
  if (orgs.length === 0) {
    return (
      <EmptyHint
        title='No orgs have granted Oxy access'
        body='When an org owner enables Oxy access on a workspace, the org shows up here.'
      />
    );
  }

  return (
    <MasterDetail
      list={
        <div className='flex h-full flex-col'>
          <PaneSearch value={search} onChange={setSearch} placeholder='Search orgs…' />
          <div className='min-h-0 flex-1 overflow-auto'>
            {filtered.map((o) => (
              <ListRow
                key={o.orgId}
                active={o.orgId === selected?.orgId}
                onClick={() => setSelectedId(o.orgId)}
                title={o.orgName}
                subtitle={o.orgSlug}
                trailing={
                  <span className='rounded bg-muted px-1.5 py-0.5 font-mono text-[11px] text-muted-foreground'>
                    {o.projects.length}
                  </span>
                }
              />
            ))}
          </div>
        </div>
      }
      detail={selected ? <OrgDetail org={selected} /> : null}
    />
  );
};

const OrgDetail = ({ org }: { org: OrgGroup }) => (
  <div className='flex h-full flex-col overflow-auto'>
    <div className='border-border border-b px-5 py-4'>
      <h2 className='font-semibold text-lg'>{org.orgName}</h2>
      <p className='font-mono text-muted-foreground text-xs'>{org.orgSlug}</p>
      <p className='mt-1 text-muted-foreground text-sm'>
        {org.projects.length} project{org.projects.length === 1 ? "" : "s"} with Oxy access
      </p>
    </div>
    <ul className='flex flex-col'>
      {org.projects.map((p) => (
        <ProjectRow key={p.workspace_id} grant={p} />
      ))}
    </ul>
  </div>
);

const ProjectRow = ({ grant }: { grant: OxyAccessGrant }) => (
  <li className='flex items-center gap-3 border-border/50 border-b px-5 py-3'>
    <span className='min-w-0 flex-1'>
      <span className='block truncate font-medium text-sm'>{grant.workspace_name}</span>
      <span className='block truncate text-muted-foreground text-xs'>
        Granted {formatGrantedAt(grant.granted_at)}
        {grant.granted_by_email ? ` by ${grant.granted_by_email}` : ""}
      </span>
    </span>
    <OpenHomeButton grant={grant} />
  </li>
);
