import { Loader2, ShieldCheck, Trash2, UserPlus } from "lucide-react";
import { type FormEvent, useState } from "react";
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
import { Input } from "@/components/ui/shadcn/input";
import { Spinner } from "@/components/ui/shadcn/spinner";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow
} from "@/components/ui/shadcn/table";
import {
  useAppAdmins,
  useCreateAppAdmin,
  useRemoveAppAdmin
} from "@/hooks/api/access/useAppAdmins";
import type { AppAdmin } from "@/types/access";

function formatGrantedAt(value: string): string {
  return new Date(value).toLocaleString();
}

/**
 * `/admin/app-admins` — OXY_OWNER-only management of the global "Oxy
 * app admin" role. Members of this list see the Customer apps surface
 * and have access to every registered customer app regardless of org
 * membership.
 *
 * Replaces the legacy `OXY_APP_ADMINS` env var allow-list. On first
 * boot the server seeds existing env entries into the table; from then
 * on, this page is the source of truth.
 */
export default function AdminAppAdmins() {
  const { data: admins = [], isPending } = useAppAdmins();
  const create = useCreateAppAdmin();
  const remove = useRemoveAppAdmin();
  const [email, setEmail] = useState("");
  const [pendingDelete, setPendingDelete] = useState<AppAdmin | null>(null);

  const onSubmit = (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    const trimmed = email.trim();
    if (!trimmed) return;
    create.mutate(trimmed, {
      onSuccess: () => setEmail("")
    });
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
        <h1 className='font-semibold text-2xl tracking-tight'>App admins</h1>
        <p className='mt-1 text-muted-foreground text-sm'>
          Members of this list have access to the Customer apps admin surface and to every
          registered customer app, regardless of org membership.
        </p>
      </div>

      <Card className='mb-6'>
        <CardContent className='p-4'>
          <form onSubmit={onSubmit} className='flex flex-col gap-3 sm:flex-row sm:items-center'>
            <div className='relative flex-1'>
              <Input
                type='email'
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                placeholder='admin@example.com'
                disabled={create.isPending}
                autoComplete='off'
              />
            </div>
            <Button type='submit' disabled={create.isPending || email.trim().length === 0}>
              {create.isPending ? (
                <>
                  <Loader2 className='size-4 animate-spin' />
                  Adding…
                </>
              ) : (
                <>
                  <UserPlus className='size-4' />
                  Add app admin
                </>
              )}
            </Button>
          </form>
        </CardContent>
      </Card>

      <Card>
        <CardContent className='p-0'>
          {isPending ? (
            <div className='flex items-center justify-center gap-2 py-16 text-muted-foreground text-sm'>
              <Spinner /> Loading…
            </div>
          ) : admins.length === 0 ? (
            <div className='flex flex-col items-center justify-center gap-2 py-16 text-muted-foreground'>
              <ShieldCheck className='size-8' />
              <p className='text-sm'>No app admins yet.</p>
              <p className='text-xs'>Add one above to grant access to the customer-apps surface.</p>
            </div>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Email</TableHead>
                  <TableHead>Source</TableHead>
                  <TableHead>Added</TableHead>
                  <TableHead className='w-12'></TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {admins.map((admin) => (
                  <TableRow key={admin.id} className='hover:bg-muted/40'>
                    <TableCell className='font-mono text-sm'>{admin.email}</TableCell>
                    <TableCell>
                      {admin.granted_by ? (
                        <Badge variant='outline'>Manual</Badge>
                      ) : (
                        <Badge variant='outline' className='text-muted-foreground'>
                          Env seed
                        </Badge>
                      )}
                    </TableCell>
                    <TableCell className='text-muted-foreground text-sm tabular-nums'>
                      {formatGrantedAt(admin.created_at)}
                    </TableCell>
                    <TableCell>
                      <Button
                        variant='ghost'
                        size='icon'
                        onClick={() => setPendingDelete(admin)}
                        aria-label={`Remove ${admin.email}`}
                      >
                        <Trash2 className='size-4 text-muted-foreground' />
                      </Button>
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
            <AlertDialogTitle>Remove app admin?</AlertDialogTitle>
            <AlertDialogDescription>
              {pendingDelete?.email} will lose access to the Customer apps surface and to apps they
              reach through this role. Re-add them anytime.
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
