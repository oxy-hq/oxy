// Crew-mode idle sign-out for an app running on an enrolled kiosk.
//
// The platform only CARRIES the number: `GET /api/frontline/device` tells a
// kiosk browser how long it may sit untouched (`idleTimeoutSeconds`, thirty
// minutes unless an admin set another), and nothing server-side watches a
// clock — only the app can tell "idle" from "someone reading the screen".
//
// Signing out goes through `signOut()`, which ends the session and NAVIGATES to
// the login page. That navigation is also what moves a kiosk onto the live
// build at every shift handoff, for free — which is why this never signs out
// sooner because an update is waiting: the normal threshold is the only one.

import * as React from "react";
import { useOxyApp } from "../custom-app/react";
import { signOut } from "./shellContext";

const ACTIVITY = ["pointerdown", "keydown", "touchstart"] as const;

export interface KioskIdleState {
  /** This browser is an enrolled kiosk. `null` until the probe answers. */
  bound: boolean | null;
  /** The kiosk's idle limit in seconds, when bound. */
  idleTimeoutSeconds: number | null;
}

export interface KioskIdleOptions {
  /** Off switch, e.g. outside crew mode. Default on. */
  enabled?: boolean;
  /** The login page to land on; `useShellContext().links.login` when known. */
  loginUrl?: string | null;
}

/** Read the kiosk probe. An unknown answer (503, network) is "not bound":
 *  the hook then does nothing, which is the safe failure for a sign-out. */
async function readDevice(fetcher: typeof fetch): Promise<KioskIdleState> {
  try {
    const res = await fetcher("/api/frontline/device", { credentials: "include" });
    if (!res.ok) return { bound: false, idleTimeoutSeconds: null };
    const body = (await res.json()) as { bound?: boolean; idleTimeoutSeconds?: unknown };
    const secs = Number(body.idleTimeoutSeconds);
    return body.bound
      ? { bound: true, idleTimeoutSeconds: Number.isFinite(secs) && secs > 0 ? secs : null }
      : { bound: false, idleTimeoutSeconds: null };
  } catch (err) {
    console.warn("[oxy] kiosk probe failed; idle sign-out stays off", err);
    return { bound: false, idleTimeoutSeconds: null };
  }
}

/**
 * Sign the shift session out after the kiosk's idle limit with no touch, key
 * or pointer input. Does nothing on a browser that is not an enrolled kiosk.
 */
export function useKioskIdle(options: KioskIdleOptions = {}): KioskIdleState {
  const { enabled = true, loginUrl } = options;
  const { fetcher } = useOxyApp();
  const [device, setDevice] = React.useState<KioskIdleState>({
    bound: null,
    idleTimeoutSeconds: null
  });

  React.useEffect(() => {
    if (!enabled) return;
    let cancelled = false;
    void readDevice(fetcher).then((d) => {
      if (!cancelled) setDevice(d);
    });
    return () => {
      cancelled = true;
    };
  }, [enabled, fetcher]);

  const secs = device.bound ? device.idleTimeoutSeconds : null;
  React.useEffect(() => {
    if (!enabled || !secs) return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const arm = () => {
      clearTimeout(timer);
      timer = setTimeout(() => {
        signOut(fetcher, loginUrl).catch((err: unknown) => {
          console.warn("[oxy] kiosk idle sign-out was refused", err);
        });
      }, secs * 1000);
    };
    for (const type of ACTIVITY) {
      window.addEventListener(type, arm, { capture: true, passive: true });
    }
    arm();
    return () => {
      clearTimeout(timer);
      for (const type of ACTIVITY) window.removeEventListener(type, arm, { capture: true });
    };
  }, [enabled, secs, fetcher, loginUrl]);

  return device;
}
