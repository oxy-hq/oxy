import { useCallback, useMemo } from "react";
import { useSearchParams } from "react-router-dom";
import {
  type AppViewState,
  type ChannelView,
  readAppViewState,
  writeAppViewState
} from "./appViewState";

/**
 * Bind the admin app view to the query string.
 *
 * Two surfaces read this: the stage (`AppDetail`) and the popped-out dossier
 * window, which is a real route with a real address bar of its own. Sharing the
 * binding is what keeps them from disagreeing about what `?section=` means — and
 * it is why the popped-out window gets linkable, back-navigable state for free
 * rather than falling back to component-local `useState`.
 *
 * `channelDefault` is per-app: an app with nothing published must read "no
 * `?channel`" as Draft. See `appViewState.ts`.
 */
export function useAppViewState(channelDefault: ChannelView) {
  const [searchParams, setSearchParams] = useSearchParams();

  const view = useMemo(
    () => readAppViewState(searchParams, { channel: channelDefault }),
    [searchParams, channelDefault]
  );

  /**
   * Write a view change.
   *
   * `push` for anything the operator chose — a device, a channel, a section —
   * so Back undoes exactly one choice. `replace` for anything the *app* did, so
   * the preview walking its own screens does not fill the admin console's Back
   * stack. Keeping those two straight is the whole point of the change this
   * belongs to: the preview has its own back/forward for its own history.
   */
  const patch = useCallback(
    (next: Partial<AppViewState>, mode: "push" | "replace" = "push") => {
      setSearchParams((prev) => writeAppViewState(prev, next, { channel: channelDefault }), {
        replace: mode === "replace"
      });
    },
    [setSearchParams, channelDefault]
  );

  return { view, patch };
}
