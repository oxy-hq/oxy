import { useMemo } from "react";
import { type AppLifecycle, deriveLifecycle, type LifecycleAppRef } from "./deriveLifecycle";
import { useAppBuilds } from "./useAppBuilds";
import {
  isNotFound,
  useAppAvailability,
  useListedAvailability,
  useStagingHeld
} from "./useCustomApps";
import { useAppHealthIndex } from "./useFleetHealth";

export type { AppLifecycle, LifecycleAppRef } from "./deriveLifecycle";

/**
 * What is live, is it OK, and what could be promoted or rolled back — for one app,
 * answered once.
 *
 * Composes the queries every panel already runs — build history, fleet health,
 * staging/held — under their existing keys (`queryKeys.customApps.*`), so mounting this
 * beside `BuildHistory` or the fleet costs no extra request: each is a second observer on
 * the same cache entry. That is the point of composing rather than adding a fifth
 * endpoint; the backend's lifecycle is being unified separately and its wire shapes do
 * not change here.
 *
 * Availability comes from the registry row (`app.availability`, which the list carries)
 * and makes no request of its own. Only a row without it — a detail response, capture
 * off, a store that did not answer — falls back to the per-app endpoint.
 *
 * The reading stays live by re-reading the app's one registry row every minute
 * (`useListedAvailability`), not by asking the availability endpoint.
 *
 * Takes the app rather than an id because the slug-addressed read (availability) and
 * `published_at` live on the row every caller already has; resolving an id to a row
 * here would be the one extra GET this hook exists to avoid.
 */
export function useAppLifecycle(app: LifecycleAppRef): AppLifecycle {
  const builds = useAppBuilds(app.id);
  const { rows: health } = useAppHealthIndex();
  const listed = useListedAvailability(app);
  const asked = useAppAvailability(
    listed ? undefined : app.org_slug,
    listed ? undefined : app.slug
  );
  const availability = listed ?? asked.data ?? null;
  // One read, no polling: the card states the count; the staging banner is what watches it.
  const held = useStagingHeld(app.id, true, { poll: false });

  const healthRow = health?.get(app.id)?.health ?? null;
  const heldUnavailable = held.isError || isNotFound(held.error);

  return useMemo(
    () =>
      deriveLifecycle({
        app,
        history: builds.data,
        health: healthRow,
        availability,
        held: held.data ?? null,
        heldUnavailable,
        isLoading: builds.isLoading,
        error: builds.error
      }),
    [
      app,
      builds.data,
      builds.isLoading,
      builds.error,
      healthRow,
      availability,
      held.data,
      heldUnavailable
    ]
  );
}
