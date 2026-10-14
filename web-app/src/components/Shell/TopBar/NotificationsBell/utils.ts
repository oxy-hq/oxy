import { appWindowName } from "@/libs/utils/appWindowName";

/** The badge text: nothing at zero, capped at "99+". */
export function badgeLabel(count: number): string | null {
  if (count <= 0) return null;
  return count > 99 ? "99+" : String(count);
}

const APP_PATH = /^\/customer-apps\/([^/?#]+)\/([^/?#]+)(?:[/?#]|$)/;

/**
 * The window a notification's link opens in: the app's own named tab for a link
 * into a custom app (see `appWindowName` — an app never opens inside HQ), or
 * `null` for a path inside this SPA.
 */
export function appWindowFor(href: string): string | null {
  const match = APP_PATH.exec(href);
  return match ? appWindowName(match[1], match[2]) : null;
}
