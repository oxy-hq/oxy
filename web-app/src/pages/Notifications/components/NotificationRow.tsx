import { InboxRow } from "@/components/Notifications/InboxRow";
import { LEVEL_ACCENT } from "@/components/Notifications/utils";
import { cn } from "@/libs/shadcn/utils";
import type { InboxItem } from "@/types/notifications";
import type { FeedEntry } from "../utils";
import { RowActions } from "./RowActions";

interface NotificationRowProps {
  entry: FeedEntry;
  /** Show the whole thread this row stands for. */
  onOpenThread: (item: InboxItem) => void;
}

/**
 * One row of the page: the same row the bell shows, then its actions and, for
 * a collapsed thread, "+N more". The accent spans both, so it is drawn here.
 */
export function NotificationRow({ entry, onOpenThread }: NotificationRowProps) {
  const { item, more } = entry;
  const accent = item.level ? LEVEL_ACCENT[item.level] : undefined;
  const hasActions = (item.actions?.length ?? 0) > 0;

  return (
    <li
      className={cn("border-b border-l-2 border-l-transparent last:border-b-0", accent?.border)}
      data-testid={`notifications-row-${item.id}`}
    >
      <InboxRow item={item} className='border-0 py-3' />
      {(hasActions || more > 0) && (
        <div className='flex flex-wrap items-center gap-2 pr-3 pb-3 pl-7'>
          <RowActions item={item} />
          {more > 0 && (
            <button
              type='button'
              onClick={() => onOpenThread(item)}
              data-testid={`notifications-thread-more-${item.id}`}
              className='rounded-md px-1.5 py-1 text-muted-foreground text-xs hover:bg-muted/50 hover:text-foreground'
            >
              +{more} more
            </button>
          )}
        </div>
      )}
    </li>
  );
}
