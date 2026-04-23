import type { GitHubCallbackResponse } from "@/types/github";

type SuccessMessage = { type: "github-callback-success" } & GitHubCallbackResponse;
type ErrorMessage = { type: "github-callback-error"; reason: string };
export type CallbackMessage = SuccessMessage | ErrorMessage;

export class GitHubCallbackCancelled extends Error {
  constructor() {
    super("Cancelled");
    this.name = "GitHubCallbackCancelled";
  }
}

const POPUP_CLOSED_POLL_MS = 500;

/**
 * Listens for a postMessage from the popup at /github/callback.
 * Resolves with the success payload (filtered by `expectedFlow`) or rejects
 * if the popup posts an error, the popup is closed before posting, or the
 * popup is null (blocked).
 */
export function waitForGitHubCallback<F extends GitHubCallbackResponse["flow"]>(
  popup: Window | null,
  expectedFlow: F
): Promise<Extract<GitHubCallbackResponse, { flow: F }>> {
  return new Promise((resolve, reject) => {
    if (!popup) {
      reject(new Error("Popup blocked. Please enable popups for this site."));
      return;
    }

    let settled = false;
    const cleanup = () => {
      window.removeEventListener("message", handler);
      window.clearInterval(watchdog);
    };

    const handler = (event: MessageEvent) => {
      if (event.origin !== window.location.origin) return;
      const data = event.data as CallbackMessage | undefined;
      if (
        !data ||
        (data.type !== "github-callback-success" && data.type !== "github-callback-error")
      ) {
        return;
      }
      settled = true;
      cleanup();
      if (data.type === "github-callback-error") {
        reject(new Error(data.reason));
        return;
      }
      if (data.flow !== expectedFlow) {
        reject(new Error(`Unexpected flow: ${data.flow}`));
        return;
      }
      resolve(data as unknown as Extract<GitHubCallbackResponse, { flow: F }>);
    };

    const watchdog = window.setInterval(() => {
      if (settled) return;
      if (popup.closed) {
        cleanup();
        reject(new GitHubCallbackCancelled());
      }
    }, POPUP_CLOSED_POLL_MS);

    window.addEventListener("message", handler);
  });
}
