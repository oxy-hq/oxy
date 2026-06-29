import {
  Activity,
  Building2,
  DollarSign,
  FolderOpen,
  Loader2,
  Pencil,
  Trash2,
  Users
} from "lucide-react";
import { useEffect, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router-dom";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle
} from "@/components/ui/shadcn/alert-dialog";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useAdminOrgUsage } from "@/hooks/api/adminMetrics/useAdminOrgUsage";
import {
  useAdminOrgDetail,
  useDeleteAdminOrg,
  useRenameAdminOrg
} from "@/hooks/api/adminTenants/useAdminOrgs";
import useCurrentUser from "@/hooks/api/users/useCurrentUser";
import ROUTES from "@/libs/utils/routes";
import { AdminDetailEyebrow, AdminDetailHeader } from "../../components/AdminDetailHeader";
import { AdminDetailStats } from "../../components/AdminDetailStats";
import { AdminDetailTabPanel, AdminDetailTabs } from "../../components/AdminDetailTabs";
import { AdminEmptyState } from "../../components/AdminEmptyState";
import { AdminLinkedList, AdminLinkedRow } from "../../components/AdminLinkedRow";
import { AdminSectionLabel } from "../../components/AdminSectionLabel";
import { AdminStatusPill } from "../../components/AdminStatusPill";
import { OrgActivityTab } from "./components/OrgActivityTab";
import { OrgBillingTab } from "./components/OrgBillingTab";
import { OrgCompilesTab } from "./components/OrgCompilesTab";
import { OrgOverviewTab } from "./components/OrgOverviewTab";
import { RoleBadge, WorkspaceStatusPill } from "./components/StatusBadges";
import { compactInt, usd } from "./format";
import { OrgSubdomainSettings } from "./OrgSubdomainSettings";
import type { OrgTabId } from "./tabs";

const USAGE_DAYS = 30;
const ALL_TABS: OrgTabId[] = [
  "overview",
  "members",
  "workspaces",
  "activity",
  "compiles",
  "billing",
  "settings"
];

const ageDays = (iso: string) =>
  Math.max(0, Math.floor((Date.now() - new Date(iso).getTime()) / 86_400_000));

/**
 * `/admin/orgs/:orgId` — the **Tenant 360**: the operator's single surface for
 * investigating one organization. Spine of the unified tenants console — the
 * org page IS the org's user list, workspace list, activity feed, cost
 * snapshot, and (for owners) billing, all cross-linked. The active tab is
 * mirrored to `?tab=` so the command palette and the overview's "needs
 * attention" feed can deep-link straight to the right section.
 */
export default function AdminOrgDetail() {
  const { orgId = "" } = useParams<{ orgId: string }>();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState("");
  const [slug, setSlug] = useState("");
  const [confirmDelete, setConfirmDelete] = useState(false);

  const { data: detail, isLoading } = useAdminOrgDetail(orgId);
  const { data: currentUser } = useCurrentUser();
  const usage = useAdminOrgUsage(orgId, USAGE_DAYS);
  const rename = useRenameAdminOrg();
  const remove = useDeleteAdminOrg();

  const isOwner = currentUser?.is_owner ?? false;

  // Tab lives in the URL so it's deep-linkable (palette, attention feed) and
  // survives a refresh. `billing` is owner-only — anyone else falls back to
  // overview rather than seeing an empty/403 panel.
  const rawTab = searchParams.get("tab") as OrgTabId | null;
  const tab: OrgTabId =
    rawTab && ALL_TABS.includes(rawTab) && (rawTab !== "billing" || isOwner) ? rawTab : "overview";
  const setTab = (next: OrgTabId) =>
    setSearchParams(
      (prev) => {
        prev.set("tab", next);
        return prev;
      },
      { replace: true }
    );

  useEffect(() => {
    if (detail) {
      setName(detail.name);
      setSlug(detail.slug);
    }
  }, [detail]);

  if (isLoading || !detail) {
    return (
      <div className='flex min-h-[60vh] items-center justify-center gap-2 text-muted-foreground text-sm'>
        <Spinner /> Loading organization…
      </div>
    );
  }

  const ownerCount = detail.owners.filter((m) => m.role === "owner").length;
  const adminCount = detail.owners.filter((m) => m.role === "admin").length;
  const readyWorkspaces = detail.workspaces.filter((w) => w.status === "ready").length;
  // Compile rows carry only workspace_id; map to names for the Compiles tab.
  const workspaceNames = Object.fromEntries(detail.workspaces.map((w) => [w.id, w.name]));

  const handleSave = () => {
    rename.mutate(
      {
        orgId: detail.id,
        body: {
          name: name !== detail.name ? name : undefined,
          slug: slug !== detail.slug ? slug : undefined
        }
      },
      { onSuccess: () => setEditing(false) }
    );
  };

  const handleDelete = () => {
    remove.mutate(detail.id, {
      onSuccess: () => {
        setConfirmDelete(false);
        navigate(ROUTES.ADMIN.ORGS);
      }
    });
  };

  return (
    <div className='mx-auto max-w-7xl space-y-8 p-6 lg:px-10 lg:py-10'>
      <AdminDetailHeader
        eyebrow={
          <AdminDetailEyebrow
            segments={[
              { label: "Admin", to: ROUTES.ADMIN.TENANTS },
              { label: "Tenants", to: ROUTES.ADMIN.TENANTS },
              { label: "Organizations", to: ROUTES.ADMIN.ORGS },
              { label: detail.name }
            ]}
          />
        }
        icon={Building2}
        title={detail.name}
        subtitle={
          <>
            <span className='font-mono text-xs'>/{detail.slug}</span>
            <span aria-hidden>·</span>
            <span>Owner {detail.owner_email ?? "—"}</span>
            <span aria-hidden>·</span>
            <span>{ageDays(detail.created_at).toLocaleString()}d old</span>
          </>
        }
        status={
          <AdminStatusPill
            tone={detail.member_count > 0 ? "ok" : "muted"}
            label={detail.member_count > 0 ? "Active" : "Empty"}
          />
        }
        actions={
          <>
            <Button variant='outline' size='sm' onClick={() => setEditing(true)}>
              <Pencil className='size-3.5' />
              Edit
            </Button>
            <Button
              variant='outline'
              size='sm'
              className='text-destructive hover:bg-destructive/10 hover:text-destructive'
              onClick={() => setConfirmDelete(true)}
            >
              <Trash2 className='size-3.5' />
              Delete
            </Button>
          </>
        }
      />

      <AdminDetailStats
        items={[
          {
            label: "Members",
            value: detail.member_count.toLocaleString(),
            sub: `${ownerCount} owner${ownerCount === 1 ? "" : "s"} · ${adminCount} admin${adminCount === 1 ? "" : "s"}`,
            icon: Users
          },
          {
            label: "Workspaces",
            value: detail.workspace_count.toLocaleString(),
            sub: detail.workspaces.length > 0 ? `${readyWorkspaces} ready` : "—",
            icon: FolderOpen
          },
          {
            label: `LLM cost · ${USAGE_DAYS}d`,
            value: usage.isLoading ? "—" : usd(usage.data?.total.cost_usd ?? 0),
            sub: usage.isLoading
              ? "loading…"
              : usage.data && usage.data.total.run_count > 0
                ? `${compactInt(usage.data.total.run_count)} runs`
                : "no activity",
            icon: DollarSign
          },
          {
            label: `Runs · ${USAGE_DAYS}d`,
            value: usage.isLoading ? "—" : compactInt(usage.data?.total.run_count ?? 0),
            sub: "agent runs",
            icon: Activity
          }
        ]}
      />

      <AdminDetailTabs<OrgTabId>
        value={tab}
        onChange={setTab}
        tabs={[
          { id: "overview", label: "Overview" },
          { id: "members", label: "Members", count: detail.member_count },
          { id: "workspaces", label: "Workspaces", count: detail.workspace_count },
          { id: "activity", label: "Activity" },
          { id: "compiles", label: "Compiles" },
          ...(isOwner ? [{ id: "billing" as const, label: "Billing" }] : []),
          { id: "settings", label: "Settings" }
        ]}
      />

      {tab === "overview" ? (
        <AdminDetailTabPanel>
          <OrgOverviewTab
            detail={detail}
            usage={usage.data}
            usageLoading={usage.isLoading}
            usageDays={USAGE_DAYS}
            onSelectTab={setTab}
          />
        </AdminDetailTabPanel>
      ) : null}

      {tab === "members" ? (
        <AdminDetailTabPanel>
          <AdminSectionLabel
            trailing={
              <span className='tabular-nums'>{detail.owners.length.toLocaleString()} total</span>
            }
          >
            Members
          </AdminSectionLabel>
          {detail.owners.length === 0 ? (
            <AdminEmptyState icon={Users} title='No members yet' />
          ) : (
            <AdminLinkedList>
              {detail.owners.map((m) => (
                <AdminLinkedRow
                  key={m.user_id}
                  to={ROUTES.ADMIN.USER_DETAIL(m.user_id)}
                  icon={Users}
                  primary={m.name || m.email}
                  secondary={m.email}
                  meta={<RoleBadge role={m.role} />}
                />
              ))}
            </AdminLinkedList>
          )}
        </AdminDetailTabPanel>
      ) : null}

      {tab === "workspaces" ? (
        <AdminDetailTabPanel>
          <AdminSectionLabel
            trailing={
              <span className='tabular-nums'>
                {detail.workspaces.length.toLocaleString()} total
              </span>
            }
          >
            Workspaces
          </AdminSectionLabel>
          {detail.workspaces.length === 0 ? (
            <AdminEmptyState icon={FolderOpen} title='No workspaces yet' />
          ) : (
            <AdminLinkedList>
              {detail.workspaces.map((w) => (
                <AdminLinkedRow
                  key={w.id}
                  to={ROUTES.ADMIN.WORKSPACE_DETAIL(w.id)}
                  icon={FolderOpen}
                  primary={w.name}
                  secondary={`Created ${new Date(w.created_at).toLocaleDateString()}`}
                  meta={<WorkspaceStatusPill status={w.status} />}
                />
              ))}
            </AdminLinkedList>
          )}
        </AdminDetailTabPanel>
      ) : null}

      {tab === "activity" ? (
        <AdminDetailTabPanel>
          <OrgActivityTab orgId={detail.id} />
        </AdminDetailTabPanel>
      ) : null}

      {tab === "compiles" ? (
        <AdminDetailTabPanel>
          <OrgCompilesTab orgId={detail.id} workspaceNames={workspaceNames} />
        </AdminDetailTabPanel>
      ) : null}

      {tab === "billing" && isOwner ? (
        <AdminDetailTabPanel>
          <OrgBillingTab orgId={detail.id} />
        </AdminDetailTabPanel>
      ) : null}

      {tab === "settings" ? (
        <AdminDetailTabPanel>
          <section className='space-y-4 rounded-lg border border-border/60 bg-card p-6'>
            <div className='space-y-1'>
              <h3 className='font-semibold text-base'>Identity</h3>
              <p className='text-muted-foreground text-xs'>
                The slug appears in every workspace URL for this organization. Changing it rewrites
                every link.
              </p>
            </div>
            <div className='grid gap-4 sm:grid-cols-2'>
              <div className='space-y-1.5'>
                <Label htmlFor='org-name'>Name</Label>
                <Input
                  id='org-name'
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  placeholder='Acme Corp'
                />
              </div>
              <div className='space-y-1.5'>
                <Label htmlFor='org-slug'>Slug</Label>
                <Input
                  id='org-slug'
                  value={slug}
                  onChange={(e) => setSlug(e.target.value)}
                  placeholder='acme'
                  className='font-mono'
                />
              </div>
            </div>
            <div className='flex justify-end gap-2 border-border/60 border-t pt-4'>
              <Button
                variant='ghost'
                size='sm'
                onClick={() => {
                  setName(detail.name);
                  setSlug(detail.slug);
                }}
                disabled={rename.isPending}
              >
                Reset
              </Button>
              <Button
                size='sm'
                onClick={handleSave}
                disabled={rename.isPending || (name === detail.name && slug === detail.slug)}
              >
                {rename.isPending ? <Loader2 className='size-3 animate-spin' /> : null}
                Save changes
              </Button>
            </div>
          </section>

          <OrgSubdomainSettings orgId={detail.id} />

          <section className='space-y-4 rounded-lg border border-destructive/40 bg-destructive/5 p-6'>
            <div className='space-y-1'>
              <h3 className='font-semibold text-base text-destructive'>Danger zone</h3>
              <p className='text-destructive/80 text-xs'>
                Removing this organization deletes every workspace, member, billing record, and
                history. This cannot be undone.
              </p>
            </div>
            <Button
              variant='outline'
              size='sm'
              className='border-destructive/40 text-destructive hover:bg-destructive/10 hover:text-destructive'
              onClick={() => setConfirmDelete(true)}
            >
              <Trash2 className='size-3.5' />
              Delete organization
            </Button>
          </section>
        </AdminDetailTabPanel>
      ) : null}

      {/* Edit dialog — kept simple; opens from the header Edit button too. */}
      <AlertDialog
        open={editing}
        onOpenChange={(o) => !o && !rename.isPending && setEditing(false)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Edit organization</AlertDialogTitle>
            <AlertDialogDescription>
              Update the public-facing name and URL slug.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <div className='space-y-3'>
            <div className='space-y-1.5'>
              <Label htmlFor='edit-org-name'>Name</Label>
              <Input id='edit-org-name' value={name} onChange={(e) => setName(e.target.value)} />
            </div>
            <div className='space-y-1.5'>
              <Label htmlFor='edit-org-slug'>Slug</Label>
              <Input
                id='edit-org-slug'
                value={slug}
                onChange={(e) => setSlug(e.target.value)}
                className='font-mono'
              />
            </div>
          </div>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={rename.isPending}>Cancel</AlertDialogCancel>
            <AlertDialogAction
              disabled={rename.isPending}
              onClick={(e) => {
                e.preventDefault();
                handleSave();
              }}
            >
              {rename.isPending ? "Saving…" : "Save"}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <AlertDialog
        open={confirmDelete}
        onOpenChange={(o) => !o && !remove.isPending && setConfirmDelete(false)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete organization?</AlertDialogTitle>
            <AlertDialogDescription>
              <strong>{detail.name}</strong> and all of its workspaces, members, and billing history
              will be removed. This action cannot be undone.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={remove.isPending}>Cancel</AlertDialogCancel>
            <AlertDialogAction
              disabled={remove.isPending}
              onClick={(e) => {
                e.preventDefault();
                handleDelete();
              }}
              className='bg-destructive text-destructive-foreground hover:bg-destructive/90'
            >
              {remove.isPending ? "Deleting…" : "Delete"}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
