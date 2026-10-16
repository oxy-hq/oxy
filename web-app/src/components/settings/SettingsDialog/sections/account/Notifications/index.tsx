import { Bell } from "lucide-react";
import type { ReactNode } from "react";
import { oxygenFirst } from "@/components/Notifications/utils";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { useNotificationCatalog } from "@/hooks/api/notifications";
import SectionHeader from "../../../components/SectionHeader";
import { PushOnThisDevice } from "./components/PushOnThisDevice";
import { SourceGroup } from "./components/SourceGroup";

const NOTE = "text-muted-foreground text-sm";

/**
 * Account → Notifications: what Oxygen and each app may send the signed-in
 * person, and where — a switch per channel at every level of each source's
 * tree. The inbox always gets everything; these switches only mute the other
 * channels. Needs no org or workspace — every preference is the caller's own.
 */
export default function AccountNotifications() {
  const catalog = useNotificationCatalog();
  const reach = catalog.data?.reach;

  let body: ReactNode;
  if (catalog.isPending) {
    body = <Skeleton className='h-24 w-full' />;
  } else if (catalog.isError || !reach) {
    body = <p className={NOTE}>Couldn't load your notification settings</p>;
  } else if (catalog.data.sources.length === 0) {
    body = (
      <p className={NOTE} data-testid='notifications-prefs-empty'>
        Nothing sends you notifications yet
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
        {oxygenFirst(catalog.data.sources).map((source) => (
          <SourceGroup key={source.app?.id ?? "oxygen"} source={source} reach={reach} />
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
