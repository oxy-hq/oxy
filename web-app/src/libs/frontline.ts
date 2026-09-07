import type { KioskDeviceRow } from "@/types/frontline";

/**
 * What a kiosk row is right now. Derived from its timestamps — the server
 * stores no state column — and shared by the Crew section (badge, count) and
 * the devices query (whether the list is worth polling).
 */
export type KioskState = "waiting" | "bound" | "expired" | "revoked";

export function kioskState(device: KioskDeviceRow, now = Date.now()): KioskState {
  if (device.revoked_at) return "revoked";
  if (device.bound_at) return "bound";
  if (device.enrol_expires_at && new Date(device.enrol_expires_at).getTime() > now) {
    return "waiting";
  }
  return "expired";
}

/**
 * True while some kiosk's enrol link is live and unspent — the window in
 * which a tablet may bind at any moment. The bind happens on that other
 * device, so nothing in the admin's browser would otherwise refetch.
 */
export function awaitingTablet(devices: KioskDeviceRow[], now = Date.now()): boolean {
  return devices.some((device) => kioskState(device, now) === "waiting");
}
