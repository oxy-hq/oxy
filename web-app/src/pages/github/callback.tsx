import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import ErrorAlert from "@/components/ui/ErrorAlert";
import { Button } from "@/components/ui/shadcn/button";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useCreateGitNamespace } from "@/hooks/api/github";
import { useGitHubAuth, validateGitHubAuthState } from "@/hooks/auth/useGitHubAuth";
import ROUTES from "@/libs/utils/routes";
import { getInstallationInfoFromUrl } from "@/utils/githubAppInstall";

export const GITHUB_OAUTH_CALLBACK_MESSAGE = "github_oauth_callback";

/**
 * Unified GitHub callback page — handles three cases:
 *
 * 1. Popup relay (namespace/install flow, any domain)
 *    GitHub redirects the popup here. We relay params to the opener via
 *    postMessage and close immediately. The opener calls its own backend.
 *
 * 2. Direct navigation — auth flow
 *    User clicked "Sign in with GitHub". State matches the value stored in
 *    sessionStorage by initiateGitHubAuth(). We exchange the code for a
 *    session token and navigate to the app.
 *
 * 3. Direct navigation — same-domain namespace/install
 *    No auth state in sessionStorage and no opener. We create the namespace
 *    directly (used during initial onboarding on the same domain).
 */
export default function GitHubCallback() {
  const [error, setError] = useState<string | null>(null);
  const hasRunRef = useRef(false);
  const navigate = useNavigate();
  const createGitNamespace = useCreateGitNamespace();
  const authMutation = useGitHubAuth();

  useEffect(() => {
    if (hasRunRef.current) return;

    async function handleCallback() {
      try {
        hasRunRef.current = true;

        const { installationId, state, code } = getInstallationInfoFromUrl();
        const urlError = new URLSearchParams(window.location.search).get("error");

        if (urlError) {
          if (window.opener) {
            window.opener.postMessage(
              { type: GITHUB_OAUTH_CALLBACK_MESSAGE, error: urlError },
              "*"
            );
            window.close();
          } else {
            navigate(`${ROUTES.AUTH.LOGIN}?error=oauth_failed`);
          }
          return;
        }

        // Case 1: Popup relay — send params back to opener and close.
        if (window.opener) {
          window.opener.postMessage(
            { type: GITHUB_OAUTH_CALLBACK_MESSAGE, installation_id: installationId, code, state },
            "*"
          );
          window.close();
          return;
        }

        // Case 2: Direct navigation — GitHub auth (sign in with GitHub).
        if (validateGitHubAuthState(state ?? null)) {
          if (!code) {
            navigate(`${ROUTES.AUTH.LOGIN}?error=no_code`);
            return;
          }
          authMutation.mutate({ code });
          return;
        }

        // Case 3: Direct navigation — same-domain namespace/install.
        if (!state) {
          setError("Missing required parameters in callback URL");
          return;
        }
        if (!installationId) {
          setError("No installation ID in callback — use the popup flow.");
          return;
        }
        await createGitNamespace.mutateAsync({
          installation_id: installationId,
          state,
          code
        });
        window.close();
      } catch (err: unknown) {
        const errorMsg = err instanceof Error ? err.message : "Unknown error";
        setError(`Unexpected error: ${errorMsg}`);
      }
    }

    handleCallback();
  }, [navigate, createGitNamespace, authMutation]);

  return (
    <div className='flex min-h-screen min-w-screen items-center justify-center'>
      <div className='text-center'>
        {error ? (
          <ErrorAlert
            title='GitHub Error'
            message={error}
            actions={
              <Button size='sm' onClick={() => navigate(ROUTES.ROOT)}>
                Go back
              </Button>
            }
          />
        ) : (
          <div className='flex flex-col items-center gap-4'>
            <Spinner className='size-8 text-primary' />
            <h2 className='font-medium text-xl'>Completing GitHub connection…</h2>
            <p className='text-muted-foreground'>Please wait a moment.</p>
          </div>
        )}
      </div>
    </div>
  );
}
