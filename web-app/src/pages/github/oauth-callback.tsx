import { useEffect, useRef } from "react";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { GitHubApiService } from "@/services/api";
import type { CallbackMessage } from "@/utils/githubCallbackMessage";

/**
 * Popup target for the GitHub OAuth + App-install flows.
 * Reads the GitHub callback query params, POSTs to BE to finalize, then
 * postMessage's the opener and closes itself.
 */
export default function GitHubOauthCallbackPage() {
  const hasRunRef = useRef(false);

  useEffect(() => {
    if (hasRunRef.current) return;
    hasRunRef.current = true;

    async function run() {
      const params = new URLSearchParams(window.location.search);
      const state = params.get("state");
      const code = params.get("code") ?? undefined;
      const installationIdRaw = params.get("installation_id");
      const installation_id = installationIdRaw ? Number(installationIdRaw) : undefined;
      const setup_action = params.get("setup_action") ?? undefined;
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
        const result = await GitHubApiService.completeCallback({
          state,
          code,
          installation_id,
          setup_action
        });
        post({ type: "github-callback-success", ...result });
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
