import { Pencil, ShieldCheck, Trash2 } from "lucide-react";
import { useState } from "react";
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
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import { Card, CardContent } from "@/components/ui/shadcn/card";
import { Spinner } from "@/components/ui/shadcn/spinner";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow
} from "@/components/ui/shadcn/table";
import { ROLE_LABELS, useAppAdmins, useRemoveAppAdmin } from "@/hooks/api/access/useAppAdmins";
import type { AppAdmin } from "@/types/access";
import { GrantForm } from "./components/GrantForm";

function formatGrantedAt(value: string): string {
  return new Date(value).toLocaleString();
}

/**
 * `/admin/app-admins` — Global Owner-only management of **platform grants**.
 *
 * A grant is `(role × scope)`: which capabilities, over which organizations.
 * `Global Admin` reaches the whole console except Billing queue and this page;
 * `App Operator` ships and develops custom apps and holds nothing else — no org
 * deletion, member management, billing, or infrastructure.
 *
 * Owner-only on purpose, and not merely because it is sensitive: this table is
 * the one surface no capability may reach. A capability that could edit grants
 * would let its holder widen their own ceiling. See
 * `internal-docs/roles-and-authorization.md`.
 *
 * Replaces the env-var allow-list (now `OXY_GLOBAL_ADMINS`, formerly
 * `OXY_APP_ADMINS`), which can only ever seed an unbounded Global Admin. On
 * first boot the server seeds existing env entries; from then on this page is
 * the source of truth.
 */
export default function AdminAppAdmins() {
  const { data: admins = [], isPending } = useAppAdmins();
  const remove = useRemoveAppAdmin();
  const [pendingDelete, setPendingDelete] = useState<AppAdmin | null>(null);
  // The row the form is editing. The table had no edit affordance at all, so changing
  // someone's role meant retyping their address into a form captioned "Grant access" —
  // a replace that reads as an add.
  //
  // `editNonce` REMOUNTS the form on every Edit press. Passing the row down and letting
  // an effect sync it looked equivalent and wasn't: pressing Edit twice on the same row
  // hands React the identical object, so the state update bails, the effect's deps never
  // change, and locally-typed edits survive a re-open. A key makes "open this row" mean
  // "start from what is stored", which is the only reading that isn't a trap.
  //
  // BOTH transitions bump the nonce. Opening without closing was the trap: clearing
  // `editing` alone removes the edit framing (and the Cancel button) while every field
  // still holds the cancelled row — leaving a destructive red "Replace grant" aimed at
  // someone the operator just declined to edit, one Enter away. Now leaving edit mode
  // remounts with `editing === undefined`, which IS the empty form, so there is no
  // separate reset path to keep in sync.
  const [editing, setEditing] = useState<AppAdmin | undefined>(undefined);
  const [editNonce, setEditNonce] = useState(0);
  const openEditor = (admin: AppAdmin) => {
    setEditing(admin);
    setEditNonce((n) => n + 1);
  };
  const closeEditor = () => {
    setEditing(undefined);
    setEditNonce((n) => n + 1);
  };

  const confirmDelete = () => {
    if (!pendingDelete) return;
    remove.mutate(pendingDelete.id, {
      onSettled: () => setPendingDelete(null)
    });
  };

  return (
    <div className='mx-auto max-w-5xl p-6'>
      <div className='mb-6'>
        <h1 className='font-semibold text-xl tracking-tight'>Staff access</h1>
        <p className='mt-1 text-muted-foreground text-xs'>
          Each grant is a role plus the organizations it reaches. Billing queue and this page stay
          reserved for Global Owners — a grant can never widen itself.
        </p>
      </div>

      <GrantForm key={editNonce} editing={editing} onCancel={closeEditor} />

      <Card>
        <CardContent className='p-0'>
          {isPending ? (
            <div className='flex items-center justify-center gap-2 py-16 text-muted-foreground text-xs'>
              <Spinner /> Loading…
            </div>
          ) : admins.length === 0 ? (
            <div className='flex flex-col items-center justify-center gap-2 py-16 text-muted-foreground'>
              <ShieldCheck className='size-8' />
              <p className='text-xs'>No staff access granted yet.</p>
              <p className='text-xs'>Grant one above to let someone into the admin console.</p>
            </div>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Email</TableHead>
                  <TableHead>Role</TableHead>
                  <TableHead>Reaches</TableHead>
                  <TableHead>Source</TableHead>
                  <TableHead>Added</TableHead>
                  <TableHead>Changed</TableHead>
                  <TableHead className='w-12'></TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {admins.map((admin) => (
                  <TableRow key={admin.id} className='hover:bg-muted/40'>
                    <TableCell className='font-mono text-xs'>{admin.email}</TableCell>
                    <TableCell>
                      <Badge
                        variant={admin.role === "global_admin" ? "default" : "outline"}
                        title={admin.capabilities.join(", ")}
                      >
                        {ROLE_LABELS[admin.role] ?? admin.role}
                      </Badge>
                    </TableCell>
                    <TableCell className='text-xs'>
                      {admin.scope_all ? (
                        <span className='text-muted-foreground'>All organizations</span>
                      ) : (
                        <span className='tabular-nums'>
                          {admin.scope_org_ids.length} organization
                          {admin.scope_org_ids.length === 1 ? "" : "s"}
                        </span>
                      )}
                    </TableCell>
                    <TableCell>
                      {admin.granted_by ? (
                        <Badge variant='outline'>Manual</Badge>
                      ) : (
                        <Badge variant='outline' className='text-muted-foreground'>
                          Env seed
                        </Badge>
                      )}
                    </TableCell>
                    <TableCell className='text-muted-foreground text-xs tabular-nums'>
                      {formatGrantedAt(admin.created_at)}
                    </TableCell>
                    <TableCell className='text-muted-foreground text-xs tabular-nums'>
                      {/* A grant is upserted in place, so "Added" alone leaves the role
                          beside it unexplained. Em-dash when it has never changed. */}
                      {admin.updated_at === admin.created_at
                        ? "—"
                        : formatGrantedAt(admin.updated_at)}
                    </TableCell>
                    <TableCell>
                      <div className='flex items-center gap-1'>
                        <Button
                          variant='ghost'
                          size='icon'
                          onClick={() => openEditor(admin)}
                          aria-label={`Edit ${admin.email}`}
                          data-testid='admin-app-admins-edit'
                        >
                          <Pencil className='size-4 text-muted-foreground' />
                        </Button>
                        <Button
                          variant='ghost'
                          size='icon'
                          onClick={() => setPendingDelete(admin)}
                          aria-label={`Remove ${admin.email}`}
                        >
                          <Trash2 className='size-4 text-muted-foreground' />
                        </Button>
                      </div>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      <AlertDialog
        open={pendingDelete !== null}
        onOpenChange={(open) => {
          if (!open && !remove.isPending) setPendingDelete(null);
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Revoke staff access?</AlertDialogTitle>
            <AlertDialogDescription>
              {pendingDelete?.email} will lose every capability this grant carries, and the
              organizations it reached. Re-grant them anytime.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={remove.isPending}>Cancel</AlertDialogCancel>
            <AlertDialogAction
              disabled={remove.isPending}
              onClick={(event) => {
                event.preventDefault();
                confirmDelete();
              }}
            >
              {remove.isPending ? "Removing…" : "Remove"}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
