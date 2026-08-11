import { useQuery } from "@tanstack/react-query";
import { AirwayConfigService } from "@/services/api/airwayConfig";
import queryKeys from "../queryKey";

/**
 * Previews which resources an admission policy would reject for one source
 * kind (`GET /admin/airway/config/{source_kind}/preview`), before it's
 * saved.
 *
 * **What it covers, and what that costs.** The endpoint is fenced to the
 * caller's platform scope: it scans every compiled pipeline of that kind in
 * every workspace whose org the caller's grant reaches, and reports the rest
 * only as `out_of_scope_pipelines`, a count. For an unbounded grant (a Global
 * Owner, or `scope_all`) that reach is the whole deployment, so this is not
 * free — never call it on mount or in a loop over source kinds. A page
 * previewing all four kinds automatically would scan every compiled pipeline
 * such a caller can see just because an operator opened the page to look, not
 * to change anything.
 *
 * The fence is also why the response is not the whole answer for a *global*
 * save: that row is fleet-wide while this scan is not, which is what
 * `computeSaveGate`'s `partial-scope` reason exists to say.
 *
 * **Lazy by design**: unlike the `options.enabled ?? true` convention used
 * elsewhere in `hooks/api/` (e.g. `useAdminWorkspacesList`), this hook
 * defaults `enabled` to `false`. The caller (a button/disclosure) must
 * explicitly opt in once an operator picks a policy to preview.
 *
 * **Both admission axes are part of the identity of a preview.** `environment`
 * is sent to the endpoint *and* is in the query key. Leaving it out of the key
 * was a real defect: previewing `permissive`/`production` (clean), then
 * changing only `environment`, left the cached body in place, `isSuccess`
 * true, and the save gate reading "confirmed clean" off a scan that never
 * considered `sandbox` — where the source factory refuses connectors outright.
 * With it in the key, that change is a cache miss and the gate goes back to
 * "unknown" until a real answer arrives.
 *
 * `sourceKind` may be `undefined` (nothing chosen yet) — the query stays
 * disabled either way, so a component can call this hook unconditionally
 * before a selection is made.
 *
 * **Pass the policy that will actually apply, not the draft.** For a workspace
 * override, a field left on "inherit" resolves to the kind's *global row*
 * (`resolve_admission` merges field by field, narrowest non-null winning), so
 * the caller must resolve `draft ?? global ?? airwayDefault` before calling —
 * omitting an inherited field makes the server preview `permissive`, and the
 * echo then matches a subject that also defaulted to `permissive`, so the gate
 * reads clean for a policy nobody scored. Both axes are plain `string` because
 * an inherited value comes from a free-text column: see
 * `AirwayConfigService.previewPolicy`.
 */
export const usePolicyPreview = (
  sourceKind: string | undefined,
  contractPolicy: string | undefined,
  environment: string | undefined,
  options: { enabled?: boolean } = {}
) =>
  useQuery({
    queryKey: queryKeys.airwayConfig.preview(sourceKind ?? "", contractPolicy, environment),
    queryFn: () =>
      AirwayConfigService.previewPolicy(sourceKind as string, contractPolicy, environment),
    enabled: !!sourceKind && (options.enabled ?? false)
  });
