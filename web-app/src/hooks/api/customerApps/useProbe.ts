import { useQuery } from "@tanstack/react-query";
import { CustomerAppsService } from "@/services/api/customerApps";
import queryKeys from "../queryKey";

/**
 * Bundle identity probe for the local-link flow. Reads `oxy-app.json`
 * + `index.html` from the picked folder and returns whatever the
 * bundle says about itself — declared name/slug and baked base path.
 *
 * The dialog uses this to lock the slug field to the manifest value
 * when present. Overriding a manifest-declared slug produces a
 * bundle whose JS chunks 404 every data fetch (the baked path won't
 * match the resolved route), so this is what stops the operator
 * from accidentally creating a guaranteed-broken app.
 *
 * Stale time short — folder probing is cheap and the operator may
 * be rebuilding the bundle in another terminal between picks.
 */
export const useProbe = (path: string, enabled = true) =>
  useQuery({
    queryKey: queryKeys.customerApps.probe(path),
    queryFn: () => CustomerAppsService.probe(path),
    enabled: enabled && !!path,
    staleTime: 5_000,
    retry: false
  });
