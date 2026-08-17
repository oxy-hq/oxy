/**
 * What to tell a developer when `/dev-login` is refused. All three refusals
 * (404 / 403 / 401) are the endpoint's normal vocabulary, and each has exactly
 * one fix, so the copy names it rather than saying "something went wrong".
 *
 * Lives beside the page rather than inside it so the copy can be pinned by a
 * test without dragging the axios client, AuthContext and the shadcn tree into
 * a suite that only needs a pure string function.
 */
export const describeDevLoginFailure = (
  httpStatus: number | undefined,
  email: string | undefined
): string => {
  switch (httpStatus) {
    case 404:
      // Two ways to land here, and the explicit var fixes both — so it leads.
      // The fallback caveat is second because the reader most likely to see a
      // genuine "not enabled" is on a release binary (`oxy start`, a Docker
      // image), where the debug-only fallback is inert.
      return "Dev sign-in is not enabled for you on this server. Set OXY_DEV_LOGIN_EMAILS and restart it. (On a debug build, an unset value falls back to OXY_GLOBAL_ADMINS — but only for requests from the server's own machine, so browsing it by LAN address lands here too.)";
    case 403:
      return email
        ? `"${email}" is not listed in OXY_DEV_LOGIN_EMAILS / OXY_GLOBAL_ADMINS on this server.`
        : "That identity is not listed in OXY_DEV_LOGIN_EMAILS / OXY_GLOBAL_ADMINS on this server.";
    case 401:
      return "That user is marked deleted, so the server refused to restore it.";
    default:
      return "Dev sign-in failed. Check the server logs.";
  }
};
