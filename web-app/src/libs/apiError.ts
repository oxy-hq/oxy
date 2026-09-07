import { isAxiosError } from "axios";

/**
 * The message an org route put in its error body. The crew and operating
 * graph routes answer `{ error }`; older org routes answer `{ message }`.
 * Neither is guaranteed, so the caller names a fallback.
 */
export function apiErrorMessage(err: unknown, fallback: string): string {
  if (isAxiosError(err)) {
    const data: unknown = err.response?.data;
    if (data && typeof data === "object") {
      const body = data as { error?: unknown; message?: unknown };
      if (typeof body.error === "string" && body.error) return body.error;
      if (typeof body.message === "string" && body.message) return body.message;
    }
    return fallback;
  }
  return err instanceof Error && err.message ? err.message : fallback;
}

export function apiStatus(err: unknown): number | undefined {
  return isAxiosError(err) ? err.response?.status : undefined;
}
