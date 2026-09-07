import type {
  FrontlineLoginRequest,
  FrontlineLoginResponse,
  FrontlineRosterResponse,
  KioskDevice
} from "@/types/frontline";
import { apiClient } from "./axios";

/**
 * Crew sign-in for frontline workers on an enrolled kiosk. All three routes are
 * unauthenticated — the kiosk cookie, not a bearer token, identifies the device —
 * and sit in `publicAPIPaths` so a wrong-PIN 401 renders inline instead of the
 * interceptor bouncing the page to `/login`.
 */
export class FrontlineService {
  /** Always 200: `{ bound: false }` when this browser holds no kiosk cookie. */
  static async deviceStatus(): Promise<KioskDevice> {
    const response = await apiClient.get("/frontline/device");
    return response.data;
  }

  /** Names to tap. Empty when the device isn't bound to `org` — never an error. */
  static async roster(org: string): Promise<FrontlineRosterResponse> {
    const response = await apiClient.get("/frontline/roster", { params: { org } });
    return response.data;
  }

  /**
   * 401 for every sign-in failure (wrong PIN, unknown identifier, locked out,
   * device not bound — one indistinguishable answer by design), 429 when the
   * org is rate-limited, 503 when the verifier is unavailable.
   */
  static async login(request: FrontlineLoginRequest): Promise<FrontlineLoginResponse> {
    const response = await apiClient.post("/frontline/login", request);
    return response.data;
  }
}
