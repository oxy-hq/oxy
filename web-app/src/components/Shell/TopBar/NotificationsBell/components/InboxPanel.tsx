import type { ReactNode } from "react";
import { useMarkAllNotificationsRead, useNotificationInbox } from "@/hooks/api/notifications";
import { InboxRow } from "./InboxRow";

const NOTE = "px-3 py-6 text-center text-muted-foreground text-sm";

/** The open inbox: a header with "Mark all read", then the list, newest first. */
export function InboxPanel({ onFollow }: { onFollow: () => void }) {
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

  return (
    <div className='flex flex-col' data-testid='notifications-panel'>
      <div className='flex items-center justify-between gap-2 border-b px-3 py-2'>
        <span className='font-medium text-sm'>Notifications</span>
        <button
          type='button'
          data-testid='notifications-mark-all-read'
          disabled={unread === 0 || markAll.isPending}
          onClick={() => markAll.mutate()}
          className='text-muted-foreground text-xs hover:text-foreground disabled:pointer-events-none disabled:opacity-50'
        >
          Mark all read
        </button>
      </div>
      <div className='max-h-96 overflow-y-auto'>{list}</div>
    </div>
  );
}
