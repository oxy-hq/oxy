import { Bell } from "lucide-react";
import { useCallback, useState } from "react";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/shadcn/popover";
import { useAuth } from "@/contexts/AuthContext";
import { useUnreadNotificationCount } from "@/hooks/api/notifications";
import { InboxPanel } from "./components/InboxPanel";
import { useInboxOpenSignals } from "./useInboxOpenSignals";
import { badgeLabel } from "./utils";

function InboxBell() {
  const [open, setOpen] = useState(false);
  const openInbox = useCallback(() => setOpen(true), []);
  useInboxOpenSignals(openInbox);
  const { data: unread = 0 } = useUnreadNotificationCount();
  const badge = badgeLabel(unread);

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type='button'
          data-testid='notifications-bell'
          aria-label={badge ? `Notifications, ${unread} unread` : "Notifications"}
          className='relative flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-sidebar-accent hover:text-foreground'
        >
          <Bell className='size-4' />
          {badge && (
            <span
              data-testid='notifications-badge'
              className='absolute -top-1 -right-1 flex h-4 min-w-4 items-center justify-center rounded-full bg-destructive px-1 font-medium text-destructive-foreground text-xs tabular-nums leading-none'
            >
              {badge}
            </span>
          )}
        </button>
      </PopoverTrigger>
      <PopoverContent align='end' className='w-80 p-0' data-testid='notifications-popover'>
        <InboxPanel onFollow={() => setOpen(false)} />
      </PopoverContent>
    </Popover>
  );
}

/**
 * The inbox bell in the top bar: an unread badge, and a popover listing the
 * signed-in person's notifications. Not in legacy local mode, which has no
 * signed-in person to notify.
 */
export function NotificationsBell() {
  const { isLocalMode } = useAuth();
  if (isLocalMode) return null;
  return <InboxBell />;
}
