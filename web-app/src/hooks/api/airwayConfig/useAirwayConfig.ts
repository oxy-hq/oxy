import { useQuery } from "@tanstack/react-query";
import { AirwayConfigService } from "@/services/api/airwayConfig";
import queryKeys from "../queryKey";

/**
 * Fetches the platform-wide Airway admission config
 * (`GET /admin/airway/config`) — every known source kind, its global row (if
 * any), and its per-workspace overrides. Staff-only, gated on the
 * `PlatformOperate` capability — not owner-only, and a scope-bounded holder
 * gets a listing fenced to the orgs their grant reaches.
 */
export const useAirwayConfig = () =>
  useQuery({
    queryKey: queryKeys.airwayConfig.config(),
    queryFn: () => AirwayConfigService.getConfig()
  });
