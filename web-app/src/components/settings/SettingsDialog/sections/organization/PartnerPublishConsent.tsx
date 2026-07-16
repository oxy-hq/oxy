import { Handshake } from "lucide-react";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { Switch } from "@/components/ui/shadcn/switch";
import {
  usePartnerPublishConsent,
  useSetPartnerPublishConsent
} from "@/hooks/api/access/usePartnerPublishConsent";

/**
 * Org setting: **let a partner publish apps into this organization**.
 *
 * Default OFF. A partner you work with can build and ship apps on your behalf, but
 * only after you turn this on — and you can turn it off at any time, which stops
 * their next publish immediately. Modeled on the Oxy-staff access control: the
 * server is the authority on `can_manage`, so an operator viewing your org sees the
 * state but cannot flip it.
 */
export default function PartnerPublishConsent({ orgId }: { orgId: string }) {
  const { data, isPending } = usePartnerPublishConsent(orgId);
  const setConsent = useSetPartnerPublishConsent(orgId);

  const enabled = data?.enabled ?? false;
  const canManage = data?.can_manage ?? false;

  return (
    <div className='flex items-start justify-between gap-4 rounded-lg border p-4'>
      <div className='flex gap-3'>
        <Handshake className='mt-0.5 size-4 shrink-0 text-muted-foreground' />
        <div className='space-y-1'>
          <p className='font-medium text-sm'>Partner app publishing</p>
          <p className='text-muted-foreground text-xs'>
            Allow a partner to publish apps into this organization. Off by default. Turning it off
            stops their next publish right away.
          </p>
        </div>
      </div>
      {isPending ? (
        <Skeleton className='h-5 w-9' />
      ) : (
        <Switch
          checked={enabled}
          disabled={!canManage || setConsent.isPending}
          title={
            canManage ? undefined : "Only an owner or admin of this organization can change this"
          }
          onCheckedChange={(v) => setConsent.mutate(v)}
        />
      )}
    </div>
  );
}
