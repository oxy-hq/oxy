import { apiClient } from "./axios";

export interface SlackInstallationStatus {
  connected: boolean;
  team_id: string | null;
  team_name: string | null;
  installed_at: string | null;
  installed_by: string | null;
  bot_user_id: string | null;
}

export class SlackService {
  static async getInstallationStatus(orgId: string): Promise<SlackInstallationStatus> {
    const response = await apiClient.get<SlackInstallationStatus>(
      `/orgs/${orgId}/slack/installation`
    );
    return response.data;
  }

  /**
   * Authenticated XHR to fetch the Slack OAuth authorize URL, then open
   * it in a regular new tab. Keeps the Oxygen UI accessible while the
   * user authorizes in Slack.
   *
   * Subtle: `window.open` is called **synchronously** before the
   * `apiClient.post` await, then navigated once the URL is back. Calling
   * `window.open` after an `await` would consume and lose the browser's
   * user-activation token (granted to the originating click), and most
   * default popup-blocker settings would silently drop the window.
   * Pre-opening `about:blank` and then assigning `location.href` is the
   * canonical workaround.
   *
   * We can't use a bare browser navigation to the backend because JWT is
   * carried in the Authorization header via the axios interceptor —
   * full navigations don't run interceptors or attach that header,
   * which would 401 before the handler even runs.
   *
   * Returns `true` if the popup was opened (whether or not the URL has
   * landed yet); `false` if `window.open` was blocked. Caller surfaces a
   * toast in the blocked case so users can unblock and retry.
   */
  static async startInstall(orgId: string): Promise<boolean> {
    // Synchronous open consumes the user-activation token while we
    // still have it. Sever `opener` so slack.com can't reach back into
    // our origin via `window.opener` after the popup navigates.
    const popup = window.open("about:blank", "_blank");
    if (!popup) {
      return false;
    }
    try {
      popup.opener = null;
    } catch {
      // Some browsers throw on assigning to opener — non-fatal.
    }
    try {
      const response = await apiClient.post<{ url: string }>(`/orgs/${orgId}/slack/install`);
      popup.location.replace(response.data.url);
      return true;
    } catch (err) {
      // Close the empty popup so it doesn't dangle on XHR failure.
      popup.close();
      throw err;
    }
  }

  static async disconnect(orgId: string): Promise<void> {
    await apiClient.delete(`/orgs/${orgId}/slack/installation`);
  }
}
