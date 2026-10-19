/** What the prompt row shows: nothing, the offer, or how to get push on an iPhone. */
export type PushPromptState = "hidden" | "offer" | "home-screen";

/** Everything the decision reads, so it is one pure function. */
export interface PushPromptInputs {
  /** The deployment has a Web Push key. */
  configured: boolean;
  /** Service workers, the Push API and notifications are all here. */
  supported: boolean;
  /** An iPhone or iPad in a browser tab: push needs Oxygen on the Home Screen. */
  needsHomeScreen: boolean;
  permission: NotificationPermission | "unsupported";
  /** Whether this browser already holds a subscription; `undefined` until known. */
  subscribed: boolean | undefined;
  /** Dismissed on this device in the last 30 days, or just turned on. */
  dismissed: boolean;
}

/**
 * Offer push only to someone who could say yes and has not been asked: a
 * configured deployment, a browser that can, permission not yet decided, no
 * subscription, no recent "×". `denied` shows nothing here — Settings explains
 * it. An iPhone tab gets the Home Screen hint instead of a button that cannot work.
 */
export function pushPromptState(inputs: PushPromptInputs): PushPromptState {
  if (!inputs.configured || inputs.dismissed) return "hidden";
  if (inputs.needsHomeScreen) return "home-screen";
  if (!inputs.supported || inputs.permission !== "default") return "hidden";
  return inputs.subscribed === false ? "offer" : "hidden";
}

/** Where a dismissal is remembered: epoch milliseconds, this device only. */
export const DISMISSED_AT_KEY = "oxy.push-prompt.dismissed-at";

/** How long a dismissal lasts. */
export const DISMISS_FOR_MS = 30 * 24 * 60 * 60 * 1000;

/**
 * Fired on `window` when a prompt is dismissed or push is turned on, so every
 * mounted prompt (the bell's and the page's) re-reads instead of one lingering.
 */
export const PUSH_PROMPT_CHANGED = "oxy:push-prompt-changed";

/** Whether the prompt was dismissed on this device less than 30 days before `now`. */
export function isDismissed(now: number): boolean {
  let raw: string | null;
  try {
    raw = window.localStorage.getItem(DISMISSED_AT_KEY);
  } catch {
    // Storage blocked (a private window, a policy): nothing was remembered.
    return false;
  }
  const at = raw === null ? Number.NaN : Number(raw);
  return Number.isFinite(at) && now - at < DISMISS_FOR_MS;
}

/** Remember a dismissal at `now`. Where storage is blocked it lasts for this page only. */
export function rememberDismissed(now: number): void {
  try {
    window.localStorage.setItem(DISMISSED_AT_KEY, String(now));
  } catch (err) {
    console.warn("Couldn't remember dismissing the notifications prompt", err);
  }
}
