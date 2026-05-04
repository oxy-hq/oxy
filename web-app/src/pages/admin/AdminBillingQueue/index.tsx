import { Inbox, RefreshCw } from "lucide-react";
import { useState } from "react";
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import { Card, CardContent, CardHeader } from "@/components/ui/shadcn/card";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import { Spinner } from "@/components/ui/shadcn/spinner";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow
} from "@/components/ui/shadcn/table";
import { useAdminOrgs } from "@/hooks/api/billing";
import type { AdminOrgRow, BillingStatusId } from "@/services/api/billing";
import { OrgAvatar } from "./components/OrgAvatar";
import ProvisionSubscriptionDialog from "./components/ProvisionSubscriptionDialog";
import SubscriptionDetailDialog from "./components/SubscriptionDetailDialog";

const STATUS_OPTIONS: BillingStatusId[] = [
  "incomplete",
  "active",
  "past_due",
  "unpaid",
  "canceled"
];

const STATUS_VARIANT: Record<BillingStatusId, "default" | "secondary" | "destructive" | "outline"> =
  {
    incomplete: "secondary",
    active: "default",
    past_due: "destructive",
    unpaid: "destructive",
    canceled: "outline"
  };

function StatusBadge({ status }: { status: BillingStatusId }) {
  return (
    <Badge variant={STATUS_VARIANT[status]} className='gap-1.5'>
      <span className='size-1.5 rounded-full bg-current opacity-70' />
      {status}
    </Badge>
  );
}

export default function AdminBillingQueue() {
  const [status, setStatus] = useState<BillingStatusId>("incomplete");
  const { data: orgs = [], isLoading, isFetching, refetch } = useAdminOrgs(status);
  const [selected, setSelected] = useState<AdminOrgRow | null>(null);
  const [detailOrg, setDetailOrg] = useState<AdminOrgRow | null>(null);

  return (
    <div className='mx-auto max-w-5xl p-6'>
      <div className='mb-6'>
        <h1 className='font-semibold text-2xl tracking-tight'>Billing queue</h1>
        <p className='mt-1 text-muted-foreground text-sm'>
          Review organizations and provision Stripe subscriptions.
        </p>
      </div>

      <Card>
        <CardHeader className='flex-row items-center justify-between gap-2 space-y-0 border-b py-4'>
          <div className='flex items-center gap-3'>
            <span className='text-muted-foreground text-sm'>Status</span>
            <Select value={status} onValueChange={(v) => setStatus(v as BillingStatusId)}>
              <SelectTrigger className='w-40'>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {STATUS_OPTIONS.map((s) => (
                  <SelectItem key={s} value={s}>
                    {s}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            {!isLoading ? (
              <span className='text-muted-foreground text-sm'>
                {orgs.length} {orgs.length === 1 ? "org" : "orgs"}
              </span>
            ) : null}
          </div>
          <Button variant='outline' size='sm' onClick={() => refetch()} disabled={isFetching}>
            <RefreshCw className={`size-4 ${isFetching ? "animate-spin" : ""}`} />
            Refresh
          </Button>
        </CardHeader>
        <CardContent className='p-0'>
          {isLoading ? (
            <div className='flex items-center justify-center gap-2 py-16 text-muted-foreground text-sm'>
              <Spinner /> Loading…
            </div>
          ) : orgs.length === 0 ? (
            <div className='flex flex-col items-center justify-center gap-2 py-16 text-muted-foreground'>
              <Inbox className='size-8' />
              <p className='text-sm'>No orgs in this status.</p>
            </div>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Org</TableHead>
                  <TableHead>Owner</TableHead>
                  <TableHead>Created</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead className='text-right'>Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {orgs.map((org) => (
                  <TableRow key={org.id} className='hover:bg-muted/40'>
                    <TableCell>
                      <div className='flex items-center gap-3'>
                        <OrgAvatar name={org.name} />
                        <div className='flex flex-col'>
                          <span className='font-medium'>{org.name}</span>
                          <span className='text-muted-foreground text-xs'>/{org.slug}</span>
                        </div>
                      </div>
                    </TableCell>
                    <TableCell className='text-muted-foreground text-sm'>
                      {org.owner_email ?? "—"}
                    </TableCell>
                    <TableCell className='text-muted-foreground text-sm tabular-nums'>
                      {new Date(org.created_at).toLocaleString()}
                    </TableCell>
                    <TableCell>
                      <StatusBadge status={org.status} />
                    </TableCell>
                    <TableCell className='space-x-2 text-right'>
                      {org.status === "incomplete" ? (
                        <Button size='sm' onClick={() => setSelected(org)}>
                          Provision
                        </Button>
                      ) : null}
                      {org.stripe_subscription_id ? (
                        <Button size='sm' variant='outline' onClick={() => setDetailOrg(org)}>
                          Subscription
                        </Button>
                      ) : null}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      {selected ? (
        <ProvisionSubscriptionDialog
          org={selected}
          onClose={() => setSelected(null)}
          onSuccess={() => {
            setSelected(null);
            refetch();
          }}
        />
      ) : null}

      {detailOrg ? (
        <SubscriptionDetailDialog org={detailOrg} onClose={() => setDetailOrg(null)} />
      ) : null}
    </div>
  );
}
