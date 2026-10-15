/**
 * `?notifications=open` opens the inbox once and is stripped, so a refresh does not
 * reopen it. It is the URL a push notification opens when no product tab is around
 * (`public/oxy-push-sw.js`); the post-login and org dispatchers carry it through.
 */
export const NOTIFICATIONS_PARAM = "notifications";

/**
 * Where the inbox is a popover under the bell. Narrower — a phone — it is a
 * full-screen sheet instead. Tailwind's `sm`, where the top bar itself folds
 * down to icons. Written as a `min-width` so the desktop branch is the one that
 * matches (and the one the test setup's always-matching `matchMedia` picks).
 */
export const POPOVER_MEDIA_QUERY = "(min-width: 640px)";
