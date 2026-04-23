import { useEffect, useRef } from "react";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { validateGitHubAuthState } from "@/hooks/auth/useGitHubAuth";
import { AuthService, GitHubApiService } from "@/services/api";
import type { CallbackMessage } from "@/utils/githubCallbackMessage";

/**
 * Popup target for all GitHub OAuth + App-install flows. Inspects the URL
 * `state` to decide which backend endpoint to call, then postMessages the
 * opener and closes itself.
 *
 *  - state matches the client-stored login state → `POST /auth/github`
 *    (sign-in flow, returns JWT + user + orgs)
 *  - otherwise → `POST /user/github/callback`
 *    (authenticated account-connect / app-install flow)
 */
export default function GitHubCallback() {
  const hasRunRef = useRef(false);

  useEffect(() => {
    if (hasRunRef.current) return;
    hasRunRef.current = true;

    async function run() {
      const params = new URLSearchParams(window.location.search);
      const state = params.get("state");
      const code = params.get("code") ?? undefined;
      const installationIdRaw = params.get("installation_id");
      const installationId = installationIdRaw ? Number(installationIdRaw) : undefined;
      const githubError = params.get("error");

      const post = (msg: CallbackMessage) => {
        window.opener?.postMessage(msg, window.location.origin);
      };

      if (githubError) {
        post({ type: "github-callback-error", reason: githubError });
        window.close();
        return;
      }

      if (!state) {
        post({ type: "github-callback-error", reason: "Missing state parameter" });
        window.close();
        return;
      }

      try {
        if (validateGitHubAuthState(state)) {
          if (!code) throw new Error("Missing code parameter");
          const auth = await AuthService.githubAuth({ code, state });
          post({ type: "github-callback-success", flow: "auth", auth });
        } else {
          const result = await GitHubApiService.completeCallback({
            state,
            code,
            installation_id: installationId
          });
          post({ type: "github-callback-success", ...result });
        }
      } catch (err: unknown) {
        const reason = err instanceof Error ? err.message : "Unknown error";
        post({ type: "github-callback-error", reason });
      } finally {
        window.close();
      }
    }

    void run();
  }, []);

  return (
    <div className='flex min-h-screen min-w-screen items-center justify-center'>
      <div className='flex flex-col items-center gap-4 text-center'>
        <Spinner className='size-8 text-primary' />
        <h2 className='font-medium text-xl'>Connecting GitHub…</h2>
        <p className='text-muted-foreground'>This window will close automatically.</p>
      </div>
    </div>
  );
}
