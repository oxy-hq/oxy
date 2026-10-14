import { Bell } from "lucide-react";
import type { ReactNode } from "react";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { useNotificationPreferences } from "@/hooks/api/notifications";
import SectionHeader from "../../../components/SectionHeader";
import { AppKinds } from "./components/AppKinds";
import { PushOnThisDevice } from "./components/PushOnThisDevice";

const NOTE = "text-muted-foreground text-sm";

/**
 * Account → Notifications: what each app may send the signed-in person, and where.
 * The inbox always gets everything; these switches only mute the other channels.
 * Needs no org or workspace — every preference is the caller's own.
 */
export default function AccountNotifications() {
  const prefs = useNotificationPreferences();
  const reach = prefs.data?.reach;

  let body: ReactNode;
  if (prefs.isPending) {
    body = <Skeleton className='h-24 w-full' />;
  } else if (prefs.isError || !reach) {
    body = <p className={NOTE}>Couldn't load your notification settings</p>;
  } else if (prefs.data.apps.length === 0) {
    body = (
      <p className={NOTE} data-testid='notifications-prefs-empty'>
        No app sends you notifications yet
      </p>
    );
  } else {
    body = (
      <div className='flex flex-col gap-3'>
        {!reach.email && (
          <p className='text-muted-foreground text-xs' data-testid='notifications-no-email'>
            No email on this account
          </p>
        )}
        {prefs.data.apps.map((app) => (
          <AppKinds key={app.app.id} prefs={app} reach={reach} />
        ))}
      </div>
    );
  }

  return (
    <div className='flex flex-col gap-5' data-testid='settings-account-notifications'>
      <SectionHeader
        icon={Bell}
        title='Notifications'
        description='Your inbox gets everything. Choose what else reaches you.'
      />
      <PushOnThisDevice devices={reach?.push_devices} />
      {body}
    </div>
  );
}
