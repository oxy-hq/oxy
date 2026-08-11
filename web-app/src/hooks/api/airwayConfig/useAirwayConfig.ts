import { useQuery } from "@tanstack/react-query";
import { AirwayConfigService } from "@/services/api/airwayConfig";
import queryKeys from "../queryKey";

/**
 * Fetches the platform-wide Airway admission config
 * (`GET /admin/airway/config`) — every known source kind, its global row (if
 * any), and its per-workspace overrides. Global-Owner only.
 */
export const useAirwayConfig = () =>
  useQuery({
    queryKey: queryKeys.airwayConfig.config(),
    queryFn: () => AirwayConfigService.getConfig()
  });
