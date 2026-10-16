/** Who is signed in to this browser, as the approval card names them. */
export interface MobileSession {
  name?: string;
  email?: string;
}

const text = (value: unknown): string | undefined =>
  typeof value === "string" && value.trim() ? value : undefined;

/**
 * The browser's session, if it is *currently valid*, or `null`. `/api/user` answers the user when
 * the stored token resolves and `null` when it doesn't (an expired token is a 200 with no body).
 * Throws on a network failure; the caller treats that as no session too.
 */
export const readMobileSession = async (): Promise<MobileSession | null> => {
  const token = localStorage.getItem("auth_token");
  if (!token) return null;
  const res = await fetch("/api/user", {
    headers: { Authorization: `Bearer ${token}` },
    credentials: "include"
  });
  const user: unknown = res.ok ? await res.json() : null;
  if (!user || typeof user !== "object") return null;
  const { name, email } = user as { name?: unknown; email?: unknown };
  return { name: text(name), email: text(email) };
};

/** A full navigation to the login page. */
export const leaveTo = (url: string): void => {
  window.location.href = url;
};

/** Hand the browser to the app, leaving no history entry to come back to this page by. */
export const returnToApp = (url: string): void => {
  window.location.replace(url);
};
