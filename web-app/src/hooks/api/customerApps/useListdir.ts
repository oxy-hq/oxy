import { useQuery } from "@tanstack/react-query";
import { CustomerAppsService } from "@/services/api/customerApps";
import queryKeys from "../queryKey";

/**
 * Server-side folder picker for the local-mode "Link existing" flow.
 *
 * Local-mode only: the server returns 404 in cloud mode, which lands
 * here as an `error` and lets the dialog hide the picker entirely.
 * The empty `path` falls back to a server-chosen default landing
 * (`$OXY_STATE_DIR/customer-apps` if present, else `$HOME`) — so the
 * dialog can mount with `path=""` and let the user navigate from
 * wherever oxy lands them.
 *
 * Short staleTime keeps tab switches snappy without hiding new
 * folders the operator just created in their terminal.
 */
export const useListdir = (path: string, showHidden: boolean, enabled = true) =>
  useQuery({
    queryKey: queryKeys.customerApps.listdir(path, showHidden),
    queryFn: () => CustomerAppsService.listdir(path, showHidden),
    enabled,
    staleTime: 5_000,
    retry: false
  });
