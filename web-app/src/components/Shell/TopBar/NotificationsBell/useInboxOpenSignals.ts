import { useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import queryKeys from "@/hooks/api/queryKey";
import { INBOX_CHANGED_MESSAGE, OPEN_INBOX_MESSAGE, pushHref } from "@/libs/webPush";
import { NOTIFICATIONS_PARAM } from "./constants";

/**
 * The two ways something outside the bell opens the inbox: `?notifications=open`
 * (see `NOTIFICATIONS_PARAM`), and
 * the push worker telling an already-open tab that its notification was clicked —
 * which goes to the push's `href` when it named one (the notifications page,
 * narrowed to that kind or thread), and opens the bell when it did not. A push
 * that arrives while the tab is open also marks the badge and list stale.
 */
export function useInboxOpenSignals(openInbox: () => void) {
  const [searchParams, setSearchParams] = useSearchParams();
  const navigate = useNavigate();
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
      const data = event.data as { type?: unknown; href?: unknown } | null;
      const type = data?.type;
      if (type === OPEN_INBOX_MESSAGE) {
        const href = pushHref(data?.href);
        if (href) navigate(href);
        else openInbox();
      }
      if (type === OPEN_INBOX_MESSAGE || type === INBOX_CHANGED_MESSAGE) {
        qc.invalidateQueries({ queryKey: queryKeys.notifications.unread() });
        qc.invalidateQueries({ queryKey: queryKeys.notifications.inbox() });
      }
    };
    worker.addEventListener("message", onMessage);
    return () => worker.removeEventListener("message", onMessage);
  }, [openInbox, navigate, qc]);
}
