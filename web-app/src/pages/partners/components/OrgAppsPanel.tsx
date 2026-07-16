import { Badge } from "@/components/ui/shadcn/badge";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { Switch } from "@/components/ui/shadcn/switch";
import { usePartnerOrgApps, useSetAppPublished } from "@/hooks/api/partners";

/**
 * The custom apps in one managed org, each with a publish toggle. Rendered
 * inline under an expanded org row; only mounted when `manage_apps` is granted,
 * so the query never fires for partners without the capability. The server
 * re-checks scope + capability on every publish/unpublish.
 */
export default function OrgAppsPanel({ partnerId, orgId }: { partnerId: string; orgId: string }) {
  const { data: apps, isLoading, error } = usePartnerOrgApps(partnerId, orgId);
  const setPublished = useSetAppPublished(partnerId);

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
        <div key={a.id} className='flex items-center justify-between gap-3 px-4 py-2.5'>
          <div className='min-w-0'>
            <div className='truncate font-medium text-sm'>{a.name}</div>
            <div className='truncate text-muted-foreground text-xs'>{a.slug}</div>
          </div>
          <div className='flex shrink-0 items-center gap-2'>
            <Badge variant={a.published ? "secondary" : "outline"}>
              {a.published ? "Published" : "Unpublished"}
            </Badge>
            <Switch
              checked={a.published}
              disabled={setPublished.isPending}
              onCheckedChange={(v) => setPublished.mutate({ appId: a.id, orgId, published: v })}
              aria-label={a.published ? `Unpublish ${a.name}` : `Publish ${a.name}`}
            />
          </div>
        </div>
      ))}
    </div>
  );
}
