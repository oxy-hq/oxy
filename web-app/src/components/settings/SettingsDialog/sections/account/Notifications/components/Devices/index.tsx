import type { ReactNode } from "react";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import {
  useNotificationDevices,
  useRemoveNotificationDevice,
  useUpdateNotificationDevice
} from "@/hooks/api/notifications";
import { thisDeviceId } from "@/libs/webPush";
import { DeviceRow } from "./components/DeviceRow";
import { PushOnThisDevice } from "./components/PushOnThisDevice";
import { useReportBrowserPermission } from "./useReportBrowserPermission";

const NOTE = "px-4 py-3 text-muted-foreground text-xs";

/**
 * Every browser and phone push reaches, under the switch that adds this one.
 * The row this browser registered says "This device".
 */
export function Devices() {
  const devices = useNotificationDevices();
  const update = useUpdateNotificationDevice();
  const remove = useRemoveNotificationDevice();
  const thisId = thisDeviceId();
  useReportBrowserPermission(devices.data?.find((d) => d.id === thisId));

  let list: ReactNode;
  if (devices.isPending) {
    list = <Skeleton className='m-4 h-10' />;
  } else if (devices.isError) {
    list = <p className={NOTE}>Couldn't load your devices</p>;
  } else if (devices.data.length === 0) {
    list = (
      <p className={NOTE} data-testid='notifications-devices-empty'>
        No devices yet
      </p>
    );
  } else {
    list = devices.data.map((device) => (
      <DeviceRow
        key={device.id}
        device={device}
        thisDevice={device.id === thisId}
        onChange={(patch) => update.mutate({ id: device.id, patch })}
        onRemove={() => remove.mutate(device.id)}
      />
    ));
  }

  return (
    <div className='flex flex-col gap-2'>
      <h4 className='font-medium text-sm'>Devices</h4>
      <div className='divide-y rounded-lg border' data-testid='notifications-devices'>
        <PushOnThisDevice />
        {list}
      </div>
    </div>
  );
}
