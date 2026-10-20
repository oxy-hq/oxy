import { isAxiosError } from "axios";

type Refusal = { error?: string; retry_after?: number };

/** What each refusal code the server answers with asks the operator to do. */
const REFUSALS: Record<string, (refusal: Refusal) => string> = {
  rate_limited: (r) => `You just sent one. Try again in ${r.retry_after ?? 10} seconds.`,
  not_in_audience: () => "This app can't notify you: you aren't a member of its org.",
  unknown_kind: () => "The app's live build no longer declares this kind. Reload the page.",
  no_store: () => "This org has no notification store yet, so nothing can be sent."
};

/** What a failed test send says, from the code the server answered with. */
export const testNotificationError = (e: unknown): string => {
  if (isAxiosError(e)) {
    const refusal = (e.response?.data ?? {}) as Refusal;
    const say = refusal.error ? REFUSALS[refusal.error] : undefined;
    if (say) return say(refusal);
    if (e.response?.status === 404) return "App not found, or outside your access.";
  }
  return e instanceof Error ? e.message : "Couldn't send the test notification.";
};
