import type { InfiniteData, UseInfiniteQueryResult } from "@tanstack/react-query";
import { useMemo } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Spinner } from "@/components/ui/shadcn/spinner";
import type { InboxItem, InboxResponse } from "@/types/notifications";
import { collapseThreads, groupByDay } from "../utils";
import { NotificationRow } from "./NotificationRow";

const NOTE = "px-4 py-10 text-center text-muted-foreground text-sm";

interface NotificationListProps {
  feed: UseInfiniteQueryResult<InfiniteData<InboxResponse>>;
  unreadOnly: boolean;
  /** Viewing one thread: every item shows, none collapse. */
  inThread: boolean;
  onOpenThread: (item: InboxItem) => void;
}

/**
 * The list, newest first, under a heading per day. A thread collapses into its
 * newest item ("+N more"); older pages load on "Load more".
 */
export function NotificationList({
  feed,
  unreadOnly,
  inThread,
  onOpenThread
}: NotificationListProps) {
  const items = useMemo(() => feed.data?.pages.flatMap((page) => page.items) ?? [], [feed.data]);
  const groups = useMemo(() => {
    const entries = inThread ? items.map((item) => ({ item, more: 0 })) : collapseThreads(items);
    return groupByDay(entries, new Date());
  }, [items, inThread]);

  if (feed.isPending) {
    return (
      <div className='flex justify-center py-10' data-testid='notifications-page-loading'>
        <Spinner />
      </div>
    );
  }
  if (feed.isError) {
    return (
      <div className={NOTE} data-testid='notifications-page-error'>
        <p>Couldn't load notifications</p>
        <Button
          size='sm'
          variant='outline'
          className='mt-3'
          onClick={() => feed.refetch()}
          data-testid='notifications-page-retry'
        >
          Retry
        </Button>
      </div>
    );
  }
  if (items.length === 0) {
    return (
      <p className={NOTE} data-testid='notifications-page-empty'>
        {unreadOnly ? "You're all caught up" : "Nothing here yet"}
      </p>
    );
  }

  return (
    <div className='flex flex-col' data-testid='notifications-page-list'>
      {groups.map((group) => (
        <section
          key={group.key}
          aria-label={group.label}
          data-testid={`notifications-day-${group.key}`}
        >
          <h2 className='sticky top-0 z-10 border-b bg-background px-4 py-1.5 font-medium text-muted-foreground text-xs'>
            {group.label}
          </h2>
          <ul>
            {group.entries.map((entry) => (
              <NotificationRow key={entry.item.id} entry={entry} onOpenThread={onOpenThread} />
            ))}
          </ul>
        </section>
      ))}
      {feed.hasNextPage && (
        <div className='flex justify-center py-4'>
          <Button
            size='sm'
            variant='outline'
            disabled={feed.isFetchingNextPage}
            onClick={() => feed.fetchNextPage()}
            data-testid='notifications-page-load-more'
          >
            {feed.isFetchingNextPage && <Spinner className='size-3' />}
            Load more
          </Button>
        </div>
      )}
    </div>
  );
}
