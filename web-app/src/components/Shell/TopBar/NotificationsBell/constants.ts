/**
 * `?notifications=open` opens the inbox once and is stripped, so a refresh does not
 * reopen it. It is the URL a push notification opens when no product tab is around
 * (`public/oxy-push-sw.js`); the post-login and org dispatchers carry it through.
 */
export const NOTIFICATIONS_PARAM = "notifications";
