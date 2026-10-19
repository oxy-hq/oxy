import { useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { useNavigate } from "react-router-dom";
import queryKeys from "@/hooks/api/queryKey";
import { INBOX_CHANGED_MESSAGE, OPEN_INBOX_MESSAGE, pushHref } from "@/libs/webPush";

/**
 * The page has no bell, so it answers the push worker itself: a clicked push
 * that names an `href` lands there (this page, narrowed to its kind or thread),
 * and any push re-reads the list.
 */
export function usePushClicks() {
  const navigate = useNavigate();
  const qc = useQueryClient();

  useEffect(() => {
    const worker = typeof navigator === "undefined" ? undefined : navigator.serviceWorker;
    if (!worker) return;
    const onMessage = (event: MessageEvent) => {
      const data = event.data as { type?: unknown; href?: unknown } | null;
      if (data?.type === OPEN_INBOX_MESSAGE) {
        const href = pushHref(data.href);
        if (href) navigate(href, { replace: true });
      }
      if (data?.type === OPEN_INBOX_MESSAGE || data?.type === INBOX_CHANGED_MESSAGE) {
        qc.invalidateQueries({ queryKey: queryKeys.notifications.inbox() });
      }
    };
    worker.addEventListener("message", onMessage);
    return () => worker.removeEventListener("message", onMessage);
  }, [navigate, qc]);
}
