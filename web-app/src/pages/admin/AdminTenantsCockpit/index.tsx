import { useAdminPartners } from "@/hooks/api/adminPartners";
import AdminOrgDetail from "@/pages/admin/AdminOrgs/AdminOrgDetail";
import AdminUserDetail from "@/pages/admin/AdminUsers/AdminUserDetail";
import AdminWorkspaceDetail from "@/pages/admin/AdminWorkspaces/AdminWorkspaceDetail";
import PartnerPane from "./components/panes/PartnerPane";
import TenantHeader from "./components/TenantHeader";
import TenantMap from "./components/TenantMap";
import TenantRail from "./components/TenantRail";
import { type TenantType, useTenantSelection } from "./useTenantSelection";

/**
 * Tenant management: a relationship-first surface for orgs, partners, and users.
 * A dense rail (with partner chips) beside an inline detail dossier, plus a Map
 * view of who-manages-whom. Workspaces live one level down, inside their org.
 * Every action is inline — operators never round-trip through pages.
 */
export default function AdminTenantsCockpit() {
  const { type, id, view, partnerFilter, setType, setId, setView, focus, setPartnerFilter } =
    useTenantSelection();

  const partnerFilterName = usePartnerName(partnerFilter);

  return (
    <div
      data-testid='admin-tenants-cockpit'
      className='flex h-[calc(100vh-3.5rem)] min-h-0 flex-col'
    >
      <TenantHeader
        type={type}
        onTypeChange={setType}
        view={view}
        onViewChange={setView}
        onCreatedOrg={setId}
      />

      {view === "map" ? (
        <main className='min-h-0 flex-1 overflow-auto'>
          <TenantMap onFocus={focus} />
        </main>
      ) : (
        <div className='flex min-h-0 flex-1'>
          <aside className='relative flex w-80 shrink-0 flex-col border-r'>
            <TenantRail
              type={type}
              selectedId={id}
              onSelect={setId}
              partnerFilter={partnerFilter}
              partnerFilterName={partnerFilterName}
              onClearPartnerFilter={() => setPartnerFilter(null)}
              onPartnerChip={setPartnerFilter}
            />
          </aside>

          <main className='min-w-0 flex-1 overflow-auto'>
            <DetailStage type={type} id={id} onClose={() => setId(null)} />
          </main>
        </div>
      )}
    </div>
  );
}

function DetailStage({
  type,
  id,
  onClose
}: {
  type: TenantType;
  id: string | null;
  onClose: () => void;
}) {
  if (!id) return <EmptyStage />;
  // Orgs / users / workspaces reuse the full standalone admin detail surfaces (the
  // Tenant-360 views) inline via an `embedded` flag — one detail implementation,
  // not a second cockpit-only copy. Cross-links inside them navigate through the
  // URL, so focusing a related tenant lands back here.
  if (type === "orgs") return <AdminOrgDetail orgId={id} embedded />;
  if (type === "users") return <AdminUserDetail userId={id} embedded />;
  if (type === "workspaces") return <AdminWorkspaceDetail workspaceId={id} embedded />;
  // Partners have no standalone route yet, so they keep the inline pane.
  return <PartnerPane partnerId={id} onClose={onClose} />;
}

function usePartnerName(partnerId: string | null): string | undefined {
  const { data } = useAdminPartners();
  if (!partnerId) return undefined;
  return data?.find((p) => p.org_id === partnerId)?.name;
}

function EmptyStage() {
  return (
    <div className='flex h-full items-center justify-center p-8 text-center'>
      <p className='max-w-sm text-muted-foreground text-sm'>
        Select an entity to inspect and manage it — roles, transfers, and partnerships are all
        editable inline here.
      </p>
    </div>
  );
}
