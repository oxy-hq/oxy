import { ShieldCheck } from "lucide-react";
import { useState } from "react";
import { AppAccessBadge, AppAccessDialog } from "@/components/appAccess";
import { Button } from "@/components/ui/shadcn/button";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { usePartnerOrgApps } from "@/hooks/api/partners";
import { PartnerAppLifecycle } from "@/pages/admin/AdminCustomApps/components/AppLifecycle";

/**
 * The custom apps in one managed org, each with a publish toggle and an access
 * control. Rendered inline under an expanded org row; only mounted when
 * `manage_apps` is granted, so the query never fires for partners without the
 * capability. The server re-checks scope + capability on every write.
 *
 * Publishing and access are the same capability on purpose: both are lifecycle.
 * Neither lets a partner READ the client's data — that stays behind
 * `develop_apps`, which is a separate grant.
 */
export default function OrgAppsPanel({ partnerId, orgId }: { partnerId: string; orgId: string }) {
  const { data: apps, isLoading, error } = usePartnerOrgApps(partnerId, orgId);
  const [managing, setManaging] = useState<{ id: string; name: string } | null>(null);

  if (isLoading)
    return (
      <div className='p-3'>
        <Skeleton className='h-8 w-full' />
      </div>
    );
  if (error) return <p className='p-3 text-destructive text-sm'>Failed to load apps.</p>;
  if (!apps?.length)
    return (
      <p className='p-3 text-muted-foreground text-sm'>No custom apps in this organization.</p>
    );

  return (
    <div className='divide-y'>
      {apps.map((a) => (
        <div
          key={a.id}
          className='flex items-center justify-between gap-3 px-4 py-2.5'
          data-testid={`partner-org-app-${a.slug}`}
        >
          <div className='min-w-0'>
            <div className='truncate font-medium text-sm'>{a.name}</div>
            <div className='truncate text-muted-foreground text-xs'>{a.slug}</div>
          </div>
          <div className='flex shrink-0 items-center gap-2'>
            {/* Visibility beside publish state. A `manage_apps` partner is a
                plausible author of an app opened to the whole org with admin grants
                still on it, so this is the list where the count matters most — and
                it was the only app list with no access indicator at all. */}
            <AppAccessBadge visibility={a.visibility} grantCount={a.grant_count} />
            <Button
              variant='ghost'
              size='sm'
              className='h-6 gap-1 px-1.5 text-[11px]'
              onClick={() => setManaging({ id: a.id, name: a.name })}
              data-testid={`partner-org-app-access-${a.slug}`}
            >
              <ShieldCheck className='size-3' aria-hidden />
              Access
            </Button>
            {/* The lifecycle card in row form — the same verdict and the same
                Publish / Unpublish confirm as the staff console and the partner
                fleet, in place of a badge plus an unconfirmed Switch. */}
            <PartnerAppLifecycle app={a} partnerId={partnerId} orgId={orgId} variant='row' />
          </div>
        </div>
      ))}

      <AppAccessDialog
        open={managing !== null}
        onOpenChange={(open) => !open && setManaging(null)}
        scope={{ kind: "partner", partnerId, orgId }}
        appId={managing?.id ?? null}
        appName={managing?.name ?? ""}
      />
    </div>
  );
}
