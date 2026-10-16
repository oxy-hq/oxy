import { formatDistanceToNowStrict } from "date-fns";
import { Clock, TriangleAlert } from "lucide-react";
import { useMarkNotificationRead } from "@/hooks/api/notifications";
import { cn } from "@/libs/shadcn/utils";
import type { InboxItem } from "@/types/notifications";
import { FollowLink } from "./FollowLink";
import { LEVEL_ACCENT, sourceLine } from "./utils";

const ROW =
  "flex w-full items-start gap-2 border-b border-l-2 border-l-transparent px-3 py-2.5 text-left last:border-b-0 hover:bg-muted/50";

function when(iso: string): string {
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? "" : formatDistanceToNowStrict(date, { addSuffix: true });
}

function LevelMark({ item }: { item: InboxItem }) {
  const accent = item.level ? LEVEL_ACCENT[item.level] : undefined;
  if (!accent) return null;
  const Icon = item.level === "critical" ? TriangleAlert : Clock;
  return (
    <span className={cn("flex shrink-0 items-center", accent.text)}>
      <Icon aria-hidden className='size-3' />
      <span className='sr-only'>{accent.label}</span>
    </span>
  );
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
          <span className='flex min-w-0 items-center gap-1'>
            <LevelMark item={item} />
            <span className='truncate' data-testid='notifications-item-source'>
              {sourceLine(item)}
            </span>
          </span>
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

interface InboxRowProps {
  item: InboxItem;
  /** Called after the row's link is followed — the bell closes itself. */
  onFollow?: () => void;
  /** Merged last: the page draws its own border and accent around the row. */
  className?: string;
}

/**
 * One notification: where it came from ("<Source> · <category>"), when, what it
 * says, and an accent for a critical or time-sensitive kind. Clicking it marks
 * it read and follows its link (`FollowLink`); with no link it only marks read.
 * Shared by the bell, the phone sheet and the page.
 */
export function InboxRow({ item, onFollow, className }: InboxRowProps) {
  const markRead = useMarkNotificationRead();
  const unread = item.read_at === null;
  const markIfUnread = () => {
    if (unread) markRead.mutate(item);
  };
  const accent = item.level ? LEVEL_ACCENT[item.level] : undefined;
  const props = {
    "data-testid": `notifications-item-${item.id}`,
    "data-unread": unread,
    "data-level": item.level,
    className: cn(ROW, accent?.border, className)
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
    onFollow?.();
  };
  return (
    <FollowLink href={item.href} {...props} onClick={follow}>
      {content}
    </FollowLink>
  );
}
