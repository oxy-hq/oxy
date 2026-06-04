import { Building2, CalendarDays, FolderOpen, Loader2, Pencil, Trash2, Users } from "lucide-react";
import { useEffect, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
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
import {
  useAdminOrgDetail,
  useDeleteAdminOrg,
  useRenameAdminOrg
} from "@/hooks/api/adminTenants/useAdminOrgs";
import ROUTES from "@/libs/utils/routes";
import { AdminDetailEyebrow, AdminDetailHeader } from "../../components/AdminDetailHeader";
import { AdminDetailStats } from "../../components/AdminDetailStats";
import { AdminDetailTabPanel, AdminDetailTabs } from "../../components/AdminDetailTabs";
import { AdminEmptyState } from "../../components/AdminEmptyState";
import { AdminLinkedList, AdminLinkedRow } from "../../components/AdminLinkedRow";
import { AdminSectionLabel } from "../../components/AdminSectionLabel";
import { AdminStatusPill } from "../../components/AdminStatusPill";

type TabId = "overview" | "members" | "workspaces" | "settings";

const ROLE_TONE: Record<string, "ok" | "info" | "muted"> = {
  owner: "ok",
  admin: "info",
  member: "muted"
};

const ageDays = (iso: string) =>
  Math.max(0, Math.floor((Date.now() - new Date(iso).getTime()) / 86_400_000));

/**
 * `/admin/orgs/:orgId` — full-page operator surface for one organization.
 * Replaces the previous slide-out sheet; every related entity (members,
 * workspaces) is rendered as a clickable linked row that traverses to
 * its own detail page. This is the centerpiece of the unified tenants
 * console: the org page IS the user list scoped to that org AND the
 * workspace list scoped to that org, with cross-links wired both ways.
 */
export default function AdminOrgDetail() {
  const { orgId = "" } = useParams<{ orgId: string }>();
  const navigate = useNavigate();
  const [tab, setTab] = useState<TabId>("overview");
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState("");
  const [slug, setSlug] = useState("");
  const [confirmDelete, setConfirmDelete] = useState(false);

  const { data: detail, isLoading } = useAdminOrgDetail(orgId);
  const rename = useRenameAdminOrg();
  const remove = useDeleteAdminOrg();

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
  const age = ageDays(detail.created_at);

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
            sub:
              detail.workspaces.length > 0
                ? `${detail.workspaces.filter((w) => w.status === "ready").length} ready`
                : "—",
            icon: FolderOpen
          },
          {
            label: "Age",
            value: age.toLocaleString(),
            sub: `days · since ${new Date(detail.created_at).toLocaleDateString(undefined, {
              year: "numeric",
              month: "short"
            })}`,
            icon: CalendarDays
          }
        ]}
      />

      <AdminDetailTabs<TabId>
        value={tab}
        onChange={setTab}
        tabs={[
          { id: "overview", label: "Overview" },
          { id: "members", label: "Members", count: detail.member_count },
          { id: "workspaces", label: "Workspaces", count: detail.workspace_count },
          { id: "settings", label: "Settings" }
        ]}
      />

      {tab === "overview" ? (
        <AdminDetailTabPanel>
          <div className='grid gap-6 lg:grid-cols-2'>
            <section className='space-y-3'>
              <AdminSectionLabel
                trailing={
                  detail.owners.length > 4 ? (
                    <button
                      type='button'
                      onClick={() => setTab("members")}
                      className='font-medium text-[10px] uppercase tracking-[0.14em] hover:text-foreground'
                    >
                      View all →
                    </button>
                  ) : null
                }
              >
                Top members
              </AdminSectionLabel>
              {detail.owners.length === 0 ? (
                <AdminEmptyState
                  icon={Users}
                  title='No members yet'
                  description='Add members via the organization settings page.'
                />
              ) : (
                <AdminLinkedList>
                  {detail.owners.slice(0, 4).map((m) => (
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
            </section>

            <section className='space-y-3'>
              <AdminSectionLabel
                trailing={
                  detail.workspaces.length > 4 ? (
                    <button
                      type='button'
                      onClick={() => setTab("workspaces")}
                      className='font-medium text-[10px] uppercase tracking-[0.14em] hover:text-foreground'
                    >
                      View all →
                    </button>
                  ) : null
                }
              >
                Recent workspaces
              </AdminSectionLabel>
              {detail.workspaces.length === 0 ? (
                <AdminEmptyState
                  icon={FolderOpen}
                  title='No workspaces yet'
                  description='Workspaces appear here when members import a repository.'
                />
              ) : (
                <AdminLinkedList>
                  {detail.workspaces.slice(0, 4).map((w) => (
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
            </section>
          </div>
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

const RoleBadge = ({ role }: { role: string }) => {
  const tone = ROLE_TONE[role.toLowerCase()] ?? "muted";
  return <AdminStatusPill tone={tone} label={role} />;
};

const WorkspaceStatusPill = ({ status }: { status: string }) => {
  const map: Record<string, { tone: "ok" | "warn" | "danger" | "muted"; label: string }> = {
    ready: { tone: "ok", label: "Ready" },
    cloning: { tone: "warn", label: "Cloning" },
    failed: { tone: "danger", label: "Failed" },
    not_oxy_project: { tone: "muted", label: "Not Oxy" }
  };
  const v = map[status] ?? { tone: "muted" as const, label: status };
  return <AdminStatusPill tone={v.tone} label={v.label} />;
};
