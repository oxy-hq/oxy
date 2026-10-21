import { readTokenAsk, type TokenAsk } from "../cliAuthRequest";

/**
 * A device-code request (`oxyc … --no-browser`), as the page reads it.
 *
 * oxyc sends the server the same `/cli-auth` params it would have put in a link, and the server
 * hands them back as they were sent. So a request is read by the link's own rules
 * (`readTokenAsk`): one that names no `kind` is `oxyc login`, and anything else is the token it
 * names — or refused, exactly as the same link would be.
 */
export type DeviceAsk = { kind: "login" } | TokenAsk;

export const readDeviceAsk = (ask: Record<string, string>): DeviceAsk => {
  const params = new URLSearchParams(ask);
  return params.has("kind") ? readTokenAsk(params) : { kind: "login" };
};

/** RFC 8628's consonants: what a user code is made of. */
const USER_CODE = /^[BCDFGHJKLMNPQRSTVWXZ]{8}$/;

/**
 * A user code as a person types it — any case, with or without the dash or spaces — as
 * `BCDF-GHJK`, or `null` when it cannot be one. Read off the URL too, so a mangled link says
 * so instead of asking the server.
 */
export const cleanUserCode = (raw: string): string | null => {
  const bare = raw.replace(/[\s-]/g, "").toUpperCase();
  return USER_CODE.test(bare) ? `${bare.slice(0, 4)}-${bare.slice(4)}` : null;
};

/** This page's own URL for a code, to come back to after signing in. */
export const deviceReturnTo = (origin: string, userCode: string): string =>
  `${origin}/cli-auth/device?code=${encodeURIComponent(userCode)}`;
