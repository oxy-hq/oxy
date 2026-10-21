import { useEffect, useRef } from "react";

/**
 * Remount the Live Preview when the live build moves.
 *
 * A Make Live or rollback succeeds by moving the app's live pointer; the
 * mutation invalidates the app queries, and the refreshed row carries a new
 * `last_promoted_at`. Until this, the frame kept showing the old build — its
 * nonce bumped only on a channel flip or a manual Reload — so an operator who
 * had just rolled back looked at the build they had rolled back from.
 *
 * Fires on a CHANGE only: the first value is what the frame already loaded.
 * A promotion made elsewhere (`oxyc publish --promote`) lands the same way the
 * next time the row refetches.
 */
export function useReloadOnPromotion(
  lastPromotedAt: string | null | undefined,
  reload: () => void
): void {
  const seen = useRef(lastPromotedAt);
  useEffect(() => {
    if (seen.current === lastPromotedAt) return;
    const first = seen.current === undefined;
    seen.current = lastPromotedAt;
    // The list row the page opens from may lack the field the detail response
    // carries; its first arrival is not a promotion.
    if (!first) reload();
  }, [lastPromotedAt, reload]);
}
