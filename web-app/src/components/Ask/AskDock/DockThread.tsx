import { Thread } from "@/pages/thread";

/** The in-dock thread view — the same Thread renderer the routed page uses,
 *  with its own header suppressed (the dock supplies one). */
export function DockThread({ threadId }: { threadId: string }) {
  return <Thread key={threadId} threadId={threadId} hideHeader />;
}
