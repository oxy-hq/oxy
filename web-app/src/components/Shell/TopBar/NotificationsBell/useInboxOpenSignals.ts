import { useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { useSearchParams } from "react-router-dom";
import queryKeys from "@/hooks/api/queryKey";
import { INBOX_CHANGED_MESSAGE, OPEN_INBOX_MESSAGE } from "@/libs/webPush";
import { NOTIFICATIONS_PARAM } from "./constants";

/**
 * The two ways something outside the bell opens the inbox: `?notifications=open`
 * (see `NOTIFICATIONS_PARAM`), and
 * the push worker telling an already-open tab that its notification was clicked. A
 * push that arrives while the tab is open also marks the badge and list stale.
 */
export function useInboxOpenSignals(openInbox: () => void) {
  const [searchParams, setSearchParams] = useSearchParams();
  const qc = useQueryClient();

  useEffect(() => {
    const value = searchParams.get(NOTIFICATIONS_PARAM);
    if (value === null) return;
    if (value === "open") openInbox();
    // The functional form, as useSettingsDeepLink does: another effect in the same
    // commit may strip its own param, and two writes from one snapshot would
    // resurrect each other's deletion.
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        next.delete(NOTIFICATIONS_PARAM);
        return next;
      },
      { replace: true }
    );
  }, [searchParams, setSearchParams, openInbox]);

  useEffect(() => {
    const worker = typeof navigator === "undefined" ? undefined : navigator.serviceWorker;
    if (!worker) return;
    const onMessage = (event: MessageEvent) => {
      const type = (event.data as { type?: unknown } | null)?.type;
      if (type === OPEN_INBOX_MESSAGE) openInbox();
      if (type === OPEN_INBOX_MESSAGE || type === INBOX_CHANGED_MESSAGE) {
        qc.invalidateQueries({ queryKey: queryKeys.notifications.unread() });
        qc.invalidateQueries({ queryKey: queryKeys.notifications.inbox() });
      }
    };
    worker.addEventListener("message", onMessage);
    return () => worker.removeEventListener("message", onMessage);
  }, [openInbox, qc]);
}
