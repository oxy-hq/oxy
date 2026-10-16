/**
 * What the Oxygen mobile app asked for when it opened `/mobile-auth`, read from the query string:
 * an S256 `code_challenge`, the `redirect_uri` to return the code to, and an opaque `state`.
 *
 * The redirect URI is checked here as the server checks it (`oxygen://`, no query, fragment or
 * user info) because Cancel redirects without asking the server: a link that names any other
 * destination is refused before anything can be sent there.
 */
export type MobileAuthRequest =
  | { kind: "invalid"; reason: string }
  | { kind: "ok"; codeChallenge: string; redirectUri: string; state: string | null };

/** The one scheme the iOS and Android apps register. */
const REDIRECT_SCHEME = "oxygen";
const MAX_REDIRECT_URI_LENGTH = 512;
/** An S256 challenge: 32 bytes, base64url, no padding. */
const CHALLENGE = /^[A-Za-z0-9_-]{43}$/;
const PRINTABLE_ASCII = /^[\x21-\x7e]+$/;

/** The redirect URI as the server will accept it, or `null`. Mirrors `clean_redirect_uri`. */
export const cleanRedirectUri = (raw: string | null): string | null => {
  const uri = raw?.trim() ?? "";
  const split = uri.indexOf("://");
  if (split < 0) return null;
  const scheme = uri.slice(0, split);
  const rest = uri.slice(split + 3);
  const ok =
    scheme.toLowerCase() === REDIRECT_SCHEME &&
    uri.length <= MAX_REDIRECT_URI_LENGTH &&
    PRINTABLE_ASCII.test(uri) &&
    !/[?#@\\]/.test(rest);
  return ok ? uri : null;
};

const RESTART = "Go back to the Oxygen app and tap Sign in again.";

export const parseMobileAuthRequest = (params: URLSearchParams): MobileAuthRequest => {
  const redirectUri = cleanRedirectUri(params.get("redirect_uri"));
  if (!redirectUri) {
    return { kind: "invalid", reason: `This link doesn't come from the Oxygen app. ${RESTART}` };
  }
  const codeChallenge = (params.get("code_challenge") ?? "").trim().replace(/=+$/, "");
  if (!CHALLENGE.test(codeChallenge)) {
    return { kind: "invalid", reason: `This sign-in link is incomplete. ${RESTART}` };
  }
  return { kind: "ok", codeChallenge, redirectUri, state: params.get("state") };
};

const back = (request: Extract<MobileAuthRequest, { kind: "ok" }>, query: string): string => {
  const state = request.state === null ? "" : `&state=${encodeURIComponent(request.state)}`;
  return `${request.redirectUri}?${query}${state}`;
};

/** Where Continue sends the browser: the app, with the single-use code. */
export const codeRedirectUrl = (
  request: Extract<MobileAuthRequest, { kind: "ok" }>,
  code: string
): string => back(request, `code=${encodeURIComponent(code)}`);

/** Where Cancel sends the browser: the app, told the person declined. */
export const declinedRedirectUrl = (request: Extract<MobileAuthRequest, { kind: "ok" }>): string =>
  back(request, "error=access_denied");

/** This page again, with exactly what the app sent, so login can bring the person back. */
export const returnToUrl = (
  origin: string,
  request: Extract<MobileAuthRequest, { kind: "ok" }>
): string => {
  const params = new URLSearchParams({
    code_challenge: request.codeChallenge,
    redirect_uri: request.redirectUri
  });
  if (request.state !== null) params.set("state", request.state);
  return `${origin}/mobile-auth?${params.toString()}`;
};

export const loginUrl = (returnTo: string): string =>
  `/login?return_to=${encodeURIComponent(returnTo)}`;
