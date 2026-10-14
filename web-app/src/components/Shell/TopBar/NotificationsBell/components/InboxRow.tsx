import { formatDistanceToNowStrict } from "date-fns";
import { Link } from "react-router-dom";
import { useMarkNotificationRead } from "@/hooks/api/notifications";
import { cn } from "@/libs/shadcn/utils";
import type { InboxItem } from "@/types/notifications";
import { appWindowFor } from "../utils";

const ROW =
  "flex w-full items-start gap-2 border-b px-3 py-2.5 text-left last:border-b-0 hover:bg-muted/50";

function when(iso: string): string {
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? "" : formatDistanceToNowStrict(date, { addSuffix: true });
}

function RowContent({ item, unread }: { item: InboxItem; unread: boolean }) {
  return (
    <>
      <span
        aria-hidden
        className={cn("mt-1.5 size-2 shrink-0 rounded-full", unread ? "bg-primary" : "invisible")}
      />
      <span className='flex min-w-0 flex-1 flex-col gap-0.5'>
        <span className='flex items-center justify-between gap-2 text-muted-foreground text-xs'>
          <span className='truncate'>{item.app?.name ?? "Oxygen"}</span>
          <time dateTime={item.created_at} className='shrink-0'>
            {when(item.created_at)}
          </time>
        </span>
        {item.title === null ? (
          <span className='text-muted-foreground text-sm italic'>
            Couldn't load this notification
          </span>
        ) : (
          <>
            <span className={cn("text-sm", unread && "font-medium")}>{item.title}</span>
            {item.body && (
              <span className='line-clamp-2 text-muted-foreground text-xs'>{item.body}</span>
            )}
          </>
        )}
      </span>
    </>
  );
}

/**
 * One notification. Clicking it marks it read and follows its link: a custom app
 * in its own named tab (plain anchor, no `rel` — see `appWindowName`), a platform
 * path through the router. With no link it only marks read.
 */
export function InboxRow({ item, onFollow }: { item: InboxItem; onFollow: () => void }) {
  const markRead = useMarkNotificationRead();
  const unread = item.read_at === null;
  const markIfUnread = () => {
    if (unread) markRead.mutate(item.id);
  };
  const props = {
    "data-testid": `notifications-item-${item.id}`,
    "data-unread": unread,
    className: ROW
  };
  const content = <RowContent item={item} unread={unread} />;

  if (!item.href) {
    return (
      <button type='button' {...props} onClick={markIfUnread}>
        {content}
      </button>
    );
  }
  const follow = () => {
    markIfUnread();
    onFollow();
  };
  const target = appWindowFor(item.href);
  if (target) {
    return (
      <a href={item.href} target={target} {...props} onClick={follow}>
        {content}
        <span className='sr-only'> (opens in a new tab)</span>
      </a>
    );
  }
  return (
    <Link to={item.href} {...props} onClick={follow}>
      {content}
    </Link>
  );
}
