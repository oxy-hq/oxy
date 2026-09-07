/**
 * Frontline (crew) sign-in — restaurant staff on a shared kiosk tablet who have
 * no email and no Oxygen account. An HttpOnly kiosk cookie binds the browser to
 * one org; the worker taps a name and enters a PIN.
 */

/** `GET /frontline/device` when this browser holds no kiosk cookie. */
export interface UnboundKioskDevice {
  bound: false;
}

/** `GET /frontline/device` for an enrolled kiosk. */
export interface BoundKioskDevice {
  bound: true;
  /** Org slug — the `org` every roster read and login is scoped to. */
  org: string;
  orgName: string;
  /** The enrolled device's display name, e.g. "Front counter". */
  device: string;
  /**
   * The app this kiosk was enrolled for, or null. Still goes through the
   * return-to allowlist before the browser is sent there.
   */
  returnTo: string | null;
}

export type KioskDevice = UnboundKioskDevice | BoundKioskDevice;

export interface FrontlineStaff {
  identifier: string;
  name: string;
}

/**
 * `GET /frontline/roster?org=`. Empty — never an error — when the device isn't
 * bound to that org.
 */
export interface FrontlineRosterResponse {
  staff: FrontlineStaff[];
}

export interface FrontlineLoginRequest {
  org: string;
  identifier: string;
  pin: string;
}

/**
 * `POST /frontline/login`. The server sets the session cookie itself; the token
 * is informational on this page. A worker is not a platform user, so this
 * deliberately carries no `UserInfo` and must never feed `AuthContext.login`.
 */
export interface FrontlineLoginResponse {
  token: string;
  name: string;
  expires_in: number;
}
