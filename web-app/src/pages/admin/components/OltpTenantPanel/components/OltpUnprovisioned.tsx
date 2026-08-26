import { Database, Loader2 } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import type { useProvisionOltp } from "@/hooks/api/oltp/useAdminOltp";
import { AdminEmptyState } from "@/pages/admin/components/AdminEmptyState";

/** No `oltp_tenants` row at all — the only state where "No OLTP database" is true. */
export const OltpUnprovisioned = ({
  provision
}: {
  provision: ReturnType<typeof useProvisionOltp>;
}) => {
  const [writers, setWriters] = useState("");

  return (
    <div className='flex flex-col gap-3' data-testid='admin-org-oltp-unprovisioned'>
      <AdminEmptyState
        icon={Database}
        title='No OLTP database'
        description='One Postgres per org. Each app and pipeline gets a schema inside it, never its own database.'
      />
      <div className='flex items-center gap-2'>
        <Input
          className='h-7 text-xs'
          placeholder='Writers, comma separated — app:bookings, pipeline:toast (optional)'
          value={writers}
          onChange={(e) => setWriters(e.target.value)}
          data-testid='admin-org-oltp-writers-input'
        />
        <Button
          size='sm'
          disabled={provision.isPending}
          onClick={() =>
            provision.mutate(
              writers
                .split(",")
                .map((w) => w.trim())
                .filter(Boolean)
            )
          }
          data-testid='admin-org-oltp-provision'
        >
          {provision.isPending ? (
            <Loader2 className='size-3 animate-spin' />
          ) : (
            <Database className='size-3' />
          )}
          Provision
        </Button>
      </div>
      <p className='text-muted-foreground text-xs'>
        Creates a billable project at the provider. Safe to re-run — provisioning is idempotent, so
        a retry converges on one database rather than two.
      </p>
    </div>
  );
};
