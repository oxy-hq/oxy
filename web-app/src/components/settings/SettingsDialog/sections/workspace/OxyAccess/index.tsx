import { Loader2, Sparkles } from "lucide-react";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { Switch } from "@/components/ui/shadcn/switch";
import { useOxyAccess, useSetOxyAccess } from "@/hooks/api/access/useOxyAccess";
import useCurrentOrg from "@/stores/useCurrentOrg";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";
import SectionHeader from "../../../components/SectionHeader";

function formatGrantedAt(value: string): string {
  return new Date(value).toLocaleString();
}

/**
 * Workspace setting: "Oxy access".
 *
 * Single toggle: when on, Oxy engineers can build customer-facing apps
 * tailored to this workspace's data, and the rendered apps become
 * accessible to them. When off (default), no one outside the
 * customer's org can reach this workspace's apps.
 */
export default function OxyAccess() {
  const { workspace } = useCurrentWorkspace();
  const workspaceId = workspace?.id ?? "";
  const orgRole = useCurrentOrg((s) => s.role) ?? "member";
  const canManage = orgRole === "owner";

  const { data: status, isPending } = useOxyAccess(workspaceId);
  const set = useSetOxyAccess(workspaceId);

  if (!workspace) return null;

  const description =
    "Let Oxy engineers build tailored apps on this workspace's data. While on, Oxy staff can open and iterate on the apps registered for this workspace. Turn it off any time to revoke access.";

  const enabled = status?.enabled ?? false;

  return (
    <div className='flex flex-col gap-6'>
      <SectionHeader icon={Sparkles} title='Oxy access' description={description} />

      <div className='flex items-start gap-4 rounded-lg border bg-card p-5'>
        <div className='flex size-10 shrink-0 items-center justify-center rounded-md border bg-primary/10 text-primary'>
          <Sparkles className='size-5' />
        </div>

        <div className='flex flex-1 flex-col gap-1'>
          <p className='font-medium text-sm leading-tight'>
            Grant Oxy permission to build tailored apps based on your data
          </p>
          {isPending ? (
            <Skeleton className='mt-1 h-3.5 w-40' />
          ) : enabled && status?.granted_at ? (
            <p className='text-muted-foreground text-xs'>
              On since {formatGrantedAt(status.granted_at)}
            </p>
          ) : (
            <p className='text-muted-foreground text-xs'>
              {enabled ? "On" : "Off — no external access"}
            </p>
          )}
        </div>

        <div className='flex items-center gap-3'>
          {set.isPending && <Loader2 className='size-3.5 animate-spin text-muted-foreground' />}
          <Switch
            checked={enabled}
            disabled={!canManage || isPending || set.isPending}
            onCheckedChange={(next) => set.mutate(next)}
            aria-label='Toggle Oxy access for this workspace'
          />
        </div>
      </div>

      {!canManage && (
        <p className='text-muted-foreground text-xs'>
          Only workspace owners can change this setting.
        </p>
      )}
    </div>
  );
}
