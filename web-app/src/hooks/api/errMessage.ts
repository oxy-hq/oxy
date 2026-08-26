import { isAxiosError } from "axios";

/**
 * Best available error message for a mutation's onError toast.
 *
 * Handles the three bodies a handler can return: a JSON `{ message }` envelope
 * (the shape `api_err` produces), a **plain-string** body (a provision handler,
 * or a proxy / SPA catch-all that returns non-JSON — where `data?.message` is
 * `undefined` and a naive read falls back to "Request failed with status code
 * N"), and a non-axios `Error`. Falls back to `fallback` otherwise.
 *
 * One copy so the many custom-app / access / onboarding mutations that surface
 * server errors can't each drift into a subtly weaker version.
 */
export function errMessage(err: unknown, fallback: string): string {
  if (isAxiosError(err)) {
    const data = err.response?.data;
    if (typeof data === "string" && data) return data;
    return data?.message ?? err.message;
  }
  return err instanceof Error ? err.message : fallback;
}
