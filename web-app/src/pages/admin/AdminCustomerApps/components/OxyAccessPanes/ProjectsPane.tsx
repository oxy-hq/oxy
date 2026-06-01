import { useMemo, useState } from "react";
import { useOxyAccessGrants } from "@/hooks/api/customerApps/useOxyAccessGrants";
import type { OxyAccessGrant } from "@/types/apps";
import {
  EmptyHint,
  formatGrantedAt,
  GrantsError,
  GrantsLoading,
  ListRow,
  MasterDetail,
  OpenHomeButton,
  PaneSearch
} from "./shared";

/**
 * Projects tab: flat, searchable master-detail of every workspace that
 * granted Oxy access (org shown alongside). Right pane shows the selected
 * workspace's grant metadata + an "Open /home" jump.
 */
export const ProjectsPane = () => {
  const { data, isLoading, error } = useOxyAccessGrants();
  const grants = useMemo(
    () =>
      [...(data ?? [])].sort(
        (a, b) =>
          a.org_name.localeCompare(b.org_name) || a.workspace_name.localeCompare(b.workspace_name)
      ),
    [data]
  );
  const [search, setSearch] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    if (!q) return grants;
    return grants.filter((g) =>
      `${g.workspace_name} ${g.org_name} ${g.org_slug}`.toLowerCase().includes(q)
    );
  }, [grants, search]);

  const selected = grants.find((g) => g.workspace_id === selectedId) ?? filtered[0] ?? null;

  if (error) return <GrantsError error={error} />;
  if (isLoading) return <GrantsLoading />;
  if (grants.length === 0) {
    return (
      <EmptyHint
        title='No workspaces have granted Oxy access'
        body='Enable Oxy access on a workspace (Settings → Oxy access) and it appears here.'
      />
    );
  }

  return (
    <MasterDetail
      list={
        <div className='flex h-full flex-col'>
          <PaneSearch value={search} onChange={setSearch} placeholder='Search projects…' />
          <div className='min-h-0 flex-1 overflow-auto'>
            {filtered.map((g) => (
              <ListRow
                key={g.workspace_id}
                active={g.workspace_id === selected?.workspace_id}
                onClick={() => setSelectedId(g.workspace_id)}
                title={g.workspace_name}
                subtitle={g.org_name}
              />
            ))}
          </div>
        </div>
      }
      detail={selected ? <ProjectDetail grant={selected} /> : null}
    />
  );
};

const ProjectDetail = ({ grant }: { grant: OxyAccessGrant }) => (
  <div className='flex h-full flex-col overflow-auto'>
    <div className='flex items-start gap-3 border-border border-b px-5 py-4'>
      <div className='min-w-0 flex-1'>
        <h2 className='truncate font-semibold text-lg'>{grant.workspace_name}</h2>
        <p className='truncate text-muted-foreground text-sm'>
          {grant.org_name} <span className='font-mono text-xs'>({grant.org_slug})</span>
        </p>
      </div>
      <OpenHomeButton grant={grant} />
    </div>
    <dl className='grid grid-cols-[8rem_1fr] gap-x-3 gap-y-2 px-5 py-4 text-sm'>
      <Field label='Workspace ID' value={grant.workspace_id} mono />
      <Field label='Org ID' value={grant.org_id} mono />
      <Field label='Granted' value={formatGrantedAt(grant.granted_at)} />
      <Field label='Granted by' value={grant.granted_by_email ?? "—"} />
    </dl>
  </div>
);

const Field = ({ label, value, mono }: { label: string; value: string; mono?: boolean }) => (
  <div className='contents'>
    <dt className='text-muted-foreground'>{label}</dt>
    <dd className={mono ? "break-all font-mono text-xs" : ""}>{value}</dd>
  </div>
);
