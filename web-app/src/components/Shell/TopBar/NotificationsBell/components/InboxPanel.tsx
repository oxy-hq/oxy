import type { ReactNode } from "react";
import { Link } from "react-router-dom";
import { InboxRow } from "@/components/Notifications/InboxRow";
import { PushPrompt } from "@/components/Notifications/PushPrompt";
import { SheetTitle } from "@/components/ui/shadcn/sheet";
import { useMarkAllNotificationsRead, useNotificationInbox } from "@/hooks/api/notifications";
import { cn } from "@/libs/shadcn/utils";
import ROUTES from "@/libs/utils/routes";

const NOTE = "px-3 py-6 text-center text-muted-foreground text-sm";

interface InboxPanelProps {
  onFollow: () => void;
  /**
   * Inside the phone's full-screen sheet: the list fills the screen, the heading
   * is the sheet's title, and the header leaves room for the sheet's close button.
   */
  fullScreen?: boolean;
}

/**
 * The open inbox: a header with "Mark all read", the offer to turn on push here
 * (when it applies), the list, newest first, and "See all" — the notifications
 * page, with its categories and history.
 */
export function InboxPanel({ onFollow, fullScreen = false }: InboxPanelProps) {
  const inbox = useNotificationInbox();
  const markAll = useMarkAllNotificationsRead();
  const items = inbox.data?.items ?? [];
  const unread = inbox.data?.unread ?? 0;

  let list: ReactNode;
  if (inbox.isPending) {
    list = (
      <p className={NOTE} data-testid='notifications-loading'>
        Loading…
      </p>
    );
  } else if (inbox.isError) {
    list = (
      <p className={NOTE} data-testid='notifications-error'>
        Couldn't load notifications
      </p>
    );
  } else if (items.length === 0) {
    list = (
      <p className={NOTE} data-testid='notifications-empty'>
        You're all caught up
      </p>
    );
  } else {
    list = items.map((item) => <InboxRow key={item.id} item={item} onFollow={onFollow} />);
  }

  const Heading = fullScreen ? SheetTitle : "span";
  return (
    <div
      className={cn("flex flex-col", fullScreen && "h-full min-h-0")}
      data-testid='notifications-panel'
    >
      <div
        className={cn(
          "flex shrink-0 items-center justify-between gap-2 border-b",
          fullScreen ? "h-12 pr-12 pl-3" : "px-3 py-2"
        )}
      >
        <Heading className={cn("font-medium", fullScreen ? "text-base" : "text-sm")}>
          Notifications
        </Heading>
        <button
          type='button'
          data-testid='notifications-mark-all-read'
          disabled={unread === 0 || markAll.isPending}
          onClick={() => markAll.mutate({})}
          className={cn(
            "text-muted-foreground hover:text-foreground disabled:pointer-events-none disabled:opacity-50",
            fullScreen ? "px-2 py-1.5 text-sm" : "text-xs"
          )}
        >
          Mark all read
        </button>
      </div>
      <PushPrompt fullScreen={fullScreen} />
      <div
        className={cn("overflow-y-auto", fullScreen ? "min-h-0 flex-1" : "max-h-96")}
        data-testid='notifications-list'
      >
        {list}
      </div>
      <Link
        to={ROUTES.NOTIFICATIONS}
        onClick={onFollow}
        data-testid='notifications-see-all'
        className={cn(
          "shrink-0 border-t text-center text-muted-foreground hover:bg-muted/50 hover:text-foreground",
          fullScreen ? "px-3 py-3 text-sm" : "px-3 py-2 text-xs"
        )}
      >
        See all
      </Link>
    </div>
  );
}
