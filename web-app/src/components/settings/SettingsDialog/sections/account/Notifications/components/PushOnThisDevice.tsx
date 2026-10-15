import type { ReactNode } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { useEnablePushOnThisDevice, useVapidPublicKey } from "@/hooks/api/notifications";
import { pushNeedsHomeScreenHere, pushSupported } from "@/libs/webPush";
import { deviceCount } from "../utils";

/**
 * Turn push on for the browser in front of you. Needs both a deployment with a
 * Web Push key and a browser with service workers and the Push API; anything
 * less reads "Not available here" — except an iPhone or iPad in a browser tab,
 * where push needs Oxygen on the Home Screen first, so it says how.
 */
export function PushOnThisDevice({ devices }: { devices: number | undefined }) {
  const vapid = useVapidPublicKey();
  const enable = useEnablePushOnThisDevice();
  const publicKey = vapid.data?.configured ? vapid.data.public_key : null;

  let action: ReactNode = null;
  let hint: ReactNode = null;
  if (vapid.isPending) {
    action = <Skeleton className='h-8 w-20' />;
  } else if (publicKey && pushSupported()) {
    action = (
      <Button
        size='sm'
        variant='outline'
        disabled={enable.isPending || enable.isSuccess}
        onClick={() => enable.mutate(publicKey)}
        data-testid='notifications-push-enable'
      >
        {enable.isSuccess ? "On" : "Turn on"}
      </Button>
    );
  } else if (publicKey && pushNeedsHomeScreenHere()) {
    hint = (
      <span className='text-muted-foreground text-xs' data-testid='notifications-push-home-screen'>
        Add Oxygen to your Home Screen (Share → Add to Home Screen) to get notifications here.
      </span>
    );
  } else {
    action = (
      <span className='text-muted-foreground text-xs' data-testid='notifications-push-unavailable'>
        Not available here
      </span>
    );
  }

  return (
    <div className='flex items-center justify-between gap-4 rounded-lg border px-4 py-3'>
      <div className='flex min-w-0 flex-col gap-0.5'>
        <span className='font-medium text-sm'>Push on this device</span>
        {devices !== undefined && (
          <span className='text-muted-foreground text-xs' data-testid='notifications-push-devices'>
            {deviceCount(devices)}
          </span>
        )}
        {hint}
      </div>
      {action}
    </div>
  );
}
