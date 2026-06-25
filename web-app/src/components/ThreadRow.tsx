import { MessagesSquare } from "lucide-react";
import { Link } from "react-router-dom";
import { timeAgo } from "@/libs/utils/date";

const rowClasses =
  "flex w-full items-center gap-2 rounded px-1 py-1.5 text-left text-muted-foreground text-xs hover:text-foreground";

interface ThreadRowProps {
  title: string;
  timestamp: string;
  /** Router destination — used when `onSelect` is absent. */
  to: string;
  /** When set, the row is a button calling this instead of a link — the Ask
   *  dock loads the thread in place rather than navigating. */
  onSelect?: () => void;
}

/** One thread-list row: icon · title · relative time. A router link by
 *  default, or a button when `onSelect` is given. Shared by `RecentThreads`
 *  (launcher) and `ThreadHistory` (chat page + Ask dock). */
export function ThreadRow({ title, timestamp, to, onSelect }: ThreadRowProps) {
  const body = (
    <>
      <MessagesSquare className='size-3.5 shrink-0' />
      <span className='min-w-0 flex-1 truncate'>{title}</span>
      <span className='shrink-0 text-muted-foreground/60 text-xs'>{timeAgo(timestamp)}</span>
    </>
  );
  return onSelect ? (
    <button type='button' onClick={onSelect} data-testid='thread-row' className={rowClasses}>
      {body}
    </button>
  ) : (
    <Link to={to} data-testid='thread-row' className={rowClasses}>
      {body}
    </Link>
  );
}
