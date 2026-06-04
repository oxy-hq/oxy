import {
  Building2,
  CalendarDays,
  FolderOpen,
  Loader2,
  ShieldCheck,
  Trash2,
  User
} from "lucide-react";
import { useState } from "react";
import { useParams } from "react-router-dom";
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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import { Spinner } from "@/components/ui/shadcn/spinner";
import {
  useAdminUserDetail,
  useRemoveUserFromOrg,
  useSetUserStatus,
  useUpdateUserOrgRole
} from "@/hooks/api/adminTenants/useAdminUsers";
import ROUTES from "@/libs/utils/routes";
import type { OrgRoleId } from "@/services/api/adminTenants";
import { AdminDetailEyebrow, AdminDetailHeader } from "../../components/AdminDetailHeader";
import { AdminDetailStats } from "../../components/AdminDetailStats";
import { AdminDetailTabPanel, AdminDetailTabs } from "../../components/AdminDetailTabs";
import { AdminEmptyState } from "../../components/AdminEmptyState";
import { AdminLinkedList, AdminLinkedRow } from "../../components/AdminLinkedRow";
import { AdminSectionLabel } from "../../components/AdminSectionLabel";
import { AdminStatusPill } from "../../components/AdminStatusPill";

type TabId = "overview" | "orgs" | "workspaces" | "settings";

const ageDays = (iso: string) =>
  Math.max(0, Math.floor((Date.now() - new Date(iso).getTime()) / 86_400_000));

const relativeDays = (iso: string) => {
  const days = ageDays(iso);
  if (days === 0) return "today";
  if (days === 1) return "1 day ago";
  if (days < 30) return `${days} days ago`;
  if (days < 365) return `${Math.floor(days / 30)} months ago`;
  return `${Math.floor(days / 365)} years ago`;
};

/**
 * `/admin/users/:userId` — full-page operator surface for one user.
 * Mirror of `AdminOrgDetail` for the user axis. Cross-links every
 * org membership to its org detail page and every workspace membership
 * to its workspace detail page, so an operator chasing a permissions
 * question can traverse the tenant graph without going back to a list.
 */
export default function AdminUserDetail() {
  const { userId = "" } = useParams<{ userId: string }>();
  const [tab, setTab] = useState<TabId>("overview");
  const [confirmDeactivate, setConfirmDeactivate] = useState(false);
  const [pendingRemoveOrg, setPendingRemoveOrg] = useState<{
    orgId: string;
    orgName: string;
  } | null>(null);

  const { data: detail, isLoading } = useAdminUserDetail(userId);
  const setStatus = useSetUserStatus();
  const updateRole = useUpdateUserOrgRole();
  const removeFromOrg = useRemoveUserFromOrg();

  if (isLoading || !detail) {
    return (
      <div className='flex min-h-[60vh] items-center justify-center gap-2 text-muted-foreground text-sm'>
        <Spinner /> Loading user…
      </div>
    );
  }

  const isDeleted = detail.status === "deleted";

  const handleDeactivate = () => {
    setStatus.mutate(
      { userId: detail.id, status: "deleted" },
      { onSuccess: () => setConfirmDeactivate(false) }
    );
  };
  const handleReactivate = () => {
    setStatus.mutate({ userId: detail.id, status: "active" });
  };
  const handleRemove = () => {
    if (!pendingRemoveOrg) return;
    removeFromOrg.mutate(
      { userId: detail.id, orgId: pendingRemoveOrg.orgId },
      { onSettled: () => setPendingRemoveOrg(null) }
    );
  };

  return (
    <div className='mx-auto max-w-7xl space-y-8 p-6 lg:px-10 lg:py-10'>
      <AdminDetailHeader
        eyebrow={
          <AdminDetailEyebrow
            segments={[
              { label: "Admin", to: ROUTES.ADMIN.TENANTS },
              { label: "Tenants", to: ROUTES.ADMIN.TENANTS },
              { label: "Users", to: ROUTES.ADMIN.USERS },
              { label: detail.name || detail.email }
            ]}
          />
        }
        icon={User}
        title={detail.name || detail.email}
        subtitle={
          <>
            <span className='font-mono text-xs'>{detail.email}</span>
            {detail.is_app_admin ? (
              <span className='inline-flex items-center gap-1 text-foreground/80'>
                <ShieldCheck className='size-3.5' />
                Global admin
              </span>
            ) : null}
          </>
        }
        status={
          <AdminStatusPill
            tone={isDeleted ? "danger" : detail.email_verified ? "ok" : "warn"}
            label={isDeleted ? "Deactivated" : detail.email_verified ? "Active" : "Unverified"}
          />
        }
        actions={
          isDeleted ? (
            <Button size='sm' onClick={handleReactivate} disabled={setStatus.isPending}>
              {setStatus.isPending ? <Loader2 className='size-3.5 animate-spin' /> : null}
              Reactivate
            </Button>
          ) : (
            <Button
              variant='outline'
              size='sm'
              className='text-destructive hover:bg-destructive/10 hover:text-destructive'
              onClick={() => setConfirmDeactivate(true)}
            >
              <Trash2 className='size-3.5' />
              Deactivate
            </Button>
          )
        }
      />

      <AdminDetailStats
        items={[
          {
            label: "Organizations",
            value: detail.org_memberships.length.toLocaleString(),
            sub: `${detail.org_memberships.filter((m) => m.role === "owner").length} as owner`,
            icon: Building2
          },
          {
            label: "Workspaces",
            value: detail.workspace_memberships.length.toLocaleString(),
            sub:
              detail.workspace_memberships.length > 0 ? "direct membership" : "via org role only",
            icon: FolderOpen
          },
          {
            label: "Last login",
            value:
              detail.last_login_at && ageDays(detail.last_login_at) < 365
                ? `${ageDays(detail.last_login_at)}d`
                : "—",
            sub: detail.last_login_at ? relativeDays(detail.last_login_at) : "Never signed in",
            icon: CalendarDays
          }
        ]}
      />

      <AdminDetailTabs<TabId>
        value={tab}
        onChange={setTab}
        tabs={[
          { id: "overview", label: "Overview" },
          { id: "orgs", label: "Organizations", count: detail.org_memberships.length },
          {
            id: "workspaces",
            label: "Workspaces",
            count: detail.workspace_memberships.length
          },
          { id: "settings", label: "Access" }
        ]}
      />

      {tab === "overview" ? (
        <AdminDetailTabPanel>
          <div className='grid gap-6 lg:grid-cols-2'>
            <section className='space-y-3'>
              <AdminSectionLabel
                trailing={
                  detail.org_memberships.length > 4 ? (
                    <button
                      type='button'
                      onClick={() => setTab("orgs")}
                      className='font-medium text-[10px] uppercase tracking-[0.14em] hover:text-foreground'
                    >
                      View all →
                    </button>
                  ) : null
                }
              >
                Organizations
              </AdminSectionLabel>
              {detail.org_memberships.length === 0 ? (
                <AdminEmptyState
                  icon={Building2}
                  title='Not a member of any organization'
                  description='Add the user to an organization to grant them workspace access.'
                />
              ) : (
                <AdminLinkedList>
                  {detail.org_memberships.slice(0, 4).map((m) => (
                    <AdminLinkedRow
                      key={m.org_id}
                      to={ROUTES.ADMIN.ORG_DETAIL(m.org_id)}
                      icon={Building2}
                      primary={m.org_name}
                      secondary={`/${m.org_slug}`}
                      meta={<AdminStatusPill tone={roleTone(m.role)} label={m.role} />}
                    />
                  ))}
                </AdminLinkedList>
              )}
            </section>

            <section className='space-y-3'>
              <AdminSectionLabel
                trailing={
                  detail.workspace_memberships.length > 4 ? (
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
                Workspaces
              </AdminSectionLabel>
              {detail.workspace_memberships.length === 0 ? (
                <AdminEmptyState
                  icon={FolderOpen}
                  title='No direct workspace memberships'
                  description='User accesses workspaces through their organization role.'
                />
              ) : (
                <AdminLinkedList>
                  {detail.workspace_memberships.slice(0, 4).map((w) => (
                    <AdminLinkedRow
                      key={w.workspace_id}
                      to={ROUTES.ADMIN.WORKSPACE_DETAIL(w.workspace_id)}
                      icon={FolderOpen}
                      primary={w.workspace_name}
                      secondary={`Joined ${new Date(w.joined_at).toLocaleDateString()}`}
                      meta={<AdminStatusPill tone={roleTone(w.role)} label={w.role} />}
                    />
                  ))}
                </AdminLinkedList>
              )}
            </section>
          </div>
        </AdminDetailTabPanel>
      ) : null}

      {tab === "orgs" ? (
        <AdminDetailTabPanel>
          <AdminSectionLabel
            trailing={
              <span className='tabular-nums'>
                {detail.org_memberships.length.toLocaleString()} total
              </span>
            }
          >
            Organization memberships
          </AdminSectionLabel>
          {detail.org_memberships.length === 0 ? (
            <AdminEmptyState icon={Building2} title='Not a member of any organization' />
          ) : (
            <AdminLinkedList>
              {detail.org_memberships.map((m) => (
                <AdminLinkedRow
                  key={m.org_id}
                  to={ROUTES.ADMIN.ORG_DETAIL(m.org_id)}
                  icon={Building2}
                  primary={m.org_name}
                  secondary={`/${m.org_slug} · joined ${new Date(m.joined_at).toLocaleDateString()}`}
                  trailing={
                    <div
                      className='flex items-center gap-1'
                      onClick={(e) => {
                        // Prevent row navigation when interacting with the inline controls.
                        e.preventDefault();
                        e.stopPropagation();
                      }}
                    >
                      <Select
                        value={m.role}
                        onValueChange={(v) =>
                          updateRole.mutate({
                            userId: detail.id,
                            orgId: m.org_id,
                            role: v as OrgRoleId
                          })
                        }
                        disabled={updateRole.isPending}
                      >
                        <SelectTrigger className='h-7 w-24 text-xs'>
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value='owner'>Owner</SelectItem>
                          <SelectItem value='admin'>Admin</SelectItem>
                          <SelectItem value='member'>Member</SelectItem>
                        </SelectContent>
                      </Select>
                      <Button
                        variant='ghost'
                        size='icon'
                        className='size-7 text-muted-foreground hover:text-destructive'
                        onClick={() =>
                          setPendingRemoveOrg({ orgId: m.org_id, orgName: m.org_name })
                        }
                        aria-label={`Remove from ${m.org_name}`}
                      >
                        <Trash2 className='size-3.5' />
                      </Button>
                    </div>
                  }
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
                {detail.workspace_memberships.length.toLocaleString()} total
              </span>
            }
          >
            Workspace memberships
          </AdminSectionLabel>
          {detail.workspace_memberships.length === 0 ? (
            <AdminEmptyState
              icon={FolderOpen}
              title='No direct workspace memberships'
              description='Access flows through the organization role only.'
            />
          ) : (
            <AdminLinkedList>
              {detail.workspace_memberships.map((w) => (
                <AdminLinkedRow
                  key={w.workspace_id}
                  to={ROUTES.ADMIN.WORKSPACE_DETAIL(w.workspace_id)}
                  icon={FolderOpen}
                  primary={w.workspace_name}
                  secondary={`Joined ${new Date(w.joined_at).toLocaleDateString()}`}
                  meta={<AdminStatusPill tone={roleTone(w.role)} label={w.role} />}
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
              <h3 className='font-semibold text-base'>Account status</h3>
              <p className='text-muted-foreground text-xs'>
                Deactivated users cannot sign in. Existing org and workspace memberships are
                preserved and re-applied on reactivation.
              </p>
            </div>
            <div className='flex items-center justify-between rounded-md border border-border/60 bg-muted/30 p-4'>
              <div className='space-y-1'>
                <div className='font-medium text-sm'>
                  Status: {isDeleted ? "Deactivated" : "Active"}
                </div>
                <div className='text-muted-foreground text-xs'>
                  Email {detail.email_verified ? "verified" : "not yet verified"} · Joined{" "}
                  {new Date(detail.created_at).toLocaleDateString()}
                </div>
              </div>
              {isDeleted ? (
                <Button size='sm' onClick={handleReactivate} disabled={setStatus.isPending}>
                  {setStatus.isPending ? <Loader2 className='size-3 animate-spin' /> : null}
                  Reactivate
                </Button>
              ) : (
                <Button
                  variant='outline'
                  size='sm'
                  className='text-destructive hover:bg-destructive/10 hover:text-destructive'
                  onClick={() => setConfirmDeactivate(true)}
                  disabled={setStatus.isPending}
                >
                  <Trash2 className='size-3' />
                  Deactivate
                </Button>
              )}
            </div>
          </section>
        </AdminDetailTabPanel>
      ) : null}

      <AlertDialog
        open={confirmDeactivate}
        onOpenChange={(o) => !o && !setStatus.isPending && setConfirmDeactivate(false)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Deactivate user?</AlertDialogTitle>
            <AlertDialogDescription>
              <strong>{detail.email}</strong> will not be able to sign in until reactivated.
              Existing org and workspace memberships are preserved.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={setStatus.isPending}>Cancel</AlertDialogCancel>
            <AlertDialogAction
              disabled={setStatus.isPending}
              onClick={(e) => {
                e.preventDefault();
                handleDeactivate();
              }}
              className='bg-destructive text-destructive-foreground hover:bg-destructive/90'
            >
              {setStatus.isPending ? "Deactivating…" : "Deactivate"}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <AlertDialog
        open={pendingRemoveOrg !== null}
        onOpenChange={(o) => !o && !removeFromOrg.isPending && setPendingRemoveOrg(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Remove from {pendingRemoveOrg?.orgName}?</AlertDialogTitle>
            <AlertDialogDescription>
              The user loses access to this organization and all of its workspaces. They can be
              re-added later.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={removeFromOrg.isPending}>Cancel</AlertDialogCancel>
            <AlertDialogAction
              disabled={removeFromOrg.isPending}
              onClick={(e) => {
                e.preventDefault();
                handleRemove();
              }}
              className='bg-destructive text-destructive-foreground hover:bg-destructive/90'
            >
              {removeFromOrg.isPending ? "Removing…" : "Remove"}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}

const roleTone = (role: string): "ok" | "info" | "muted" =>
  role.toLowerCase() === "owner" ? "ok" : role.toLowerCase() === "admin" ? "info" : "muted";
