import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import ErrorAlert from "@/components/ui/ErrorAlert";
import { Button } from "@/components/ui/shadcn/button";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useGitHubAuth, validateGitHubAuthState } from "@/hooks/auth/useGitHubAuth";
import ROUTES from "@/libs/utils/routes";

/**
 * GitHub OAuth callback page — handles the direct-navigation auth flow
 * (Sign in with GitHub). The namespace/install popup flow is now handled
 * server-side; this page only processes auth code exchanges.
 */
export default function GitHubCallback() {
  const [error, setError] = useState<string | null>(null);
  const hasRunRef = useRef(false);
  const navigate = useNavigate();
  const authMutation = useGitHubAuth();

  useEffect(() => {
    if (hasRunRef.current) return;

    async function handleCallback() {
      try {
        hasRunRef.current = true;

        const params = new URLSearchParams(window.location.search);
        const urlError = params.get("error");
        const code = params.get("code") ?? undefined;
        const state = params.get("state") ?? undefined;

        if (urlError) {
          navigate(`${ROUTES.AUTH.LOGIN}?error=oauth_failed`);
          return;
        }

        // Auth flow — GitHub login (sign in with GitHub button).
        if (validateGitHubAuthState(state ?? null)) {
          if (!code || !state) {
            navigate(`${ROUTES.AUTH.LOGIN}?error=no_code`);
            return;
          }
          authMutation.mutate({ code, state });
          return;
        }

        // Unrecognised state — redirect to login.
        navigate(ROUTES.AUTH.LOGIN);
      } catch (err: unknown) {
        const errorMsg = err instanceof Error ? err.message : "Unknown error";
        setError(`Unexpected error: ${errorMsg}`);
      }
    }

    handleCallback();
  }, [navigate, authMutation]);

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
