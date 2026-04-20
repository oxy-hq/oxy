import type { AxiosError } from "axios";
import { CheckCircle2, XCircle } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle
} from "@/components/ui/shadcn/card";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useAuth } from "@/contexts/AuthContext";
import { useAcceptInvitation } from "@/hooks/api/organizations";
import { PENDING_INVITE_TOKEN_KEY } from "@/hooks/auth/postLoginRedirect";
import ROUTES from "@/libs/utils/routes";
import useCurrentOrg from "@/stores/useCurrentOrg";

type InviteState = { kind: "pending" } | { kind: "error"; error: unknown } | { kind: "success" };

export default function InvitePage() {
  const { token } = useParams<{ token: string }>();
  const navigate = useNavigate();
  const { isAuthenticated, authConfig, logout } = useAuth();
  const { mutateAsync } = useAcceptInvitation();
  const [state, setState] = useState<InviteState>({ kind: "pending" });
  const triggered = useRef(false);

  useEffect(() => {
    if (!token || triggered.current) return;

    if (authConfig.auth_enabled && !isAuthenticated()) {
      sessionStorage.setItem(PENDING_INVITE_TOKEN_KEY, token);
      navigate(ROUTES.AUTH.LOGIN, { replace: true });
      return;
    }

    triggered.current = true;
    mutateAsync(token)
      .then((org) => {
        useCurrentOrg.getState().setOrg({
          id: org.id,
          name: org.name,
          slug: org.slug,
          role: org.role
        });
        setState({ kind: "success" });
        navigate(ROUTES.ORG(org.slug).WORKSPACES, { replace: true });
      })
      .catch((err) => {
        setState({ kind: "error", error: err });
      });
  }, [token, authConfig.auth_enabled, isAuthenticated, mutateAsync, navigate]);

  if (!token) {
    return (
      <ErrorCard
        title='Invalid invitation link'
        description='This invitation link is missing or malformed.'
      />
    );
  }

  if (state.kind === "error") {
    const { status } = state.error as AxiosError | { status: "network" | "unknown" };

    if (status === 401) {
      sessionStorage.setItem(PENDING_INVITE_TOKEN_KEY, token);
      return (
        <ErrorCard
          title='Session expired'
          description='Please sign in again to accept your invitation.'
          primaryAction={{
            label: "Sign in",
            onClick: () => navigate(ROUTES.AUTH.LOGIN, { replace: true })
          }}
        />
      );
    }

    if (status === 403) {
      return (
        <ErrorCard
          title='Wrong account'
          description='This invitation was sent to a different email. Sign out and sign in with the invited address to accept it.'
          primaryAction={{
            label: "Sign out",
            onClick: () => {
              sessionStorage.setItem(PENDING_INVITE_TOKEN_KEY, token);
              logout();
            }
          }}
        />
      );
    }

    const isNetworkError = status === "network";
    const isServerError = typeof status === "number" && status >= 500;

    let title = "Invitation unavailable";
    let description = "This invitation is invalid or has expired.";

    if (isNetworkError) {
      title = "Connection error";
      description = "We couldn't reach the server. Check your connection and try again.";
    } else if (isServerError) {
      title = "Something went wrong";
      description = "The server ran into an error. Please try again in a moment.";
    } else if (status === 404) {
      description = "This invitation link is invalid or has been revoked.";
    } else if (status === 409) {
      description = "You're already a member of this organization.";
    } else if (status === 400) {
      description = "This invitation has expired or is no longer valid.";
    }

    const canRetry = isNetworkError || isServerError;
    return (
      <ErrorCard
        title={title}
        description={description}
        primaryAction={
          canRetry
            ? {
                label: "Try again",
                onClick: () => {
                  triggered.current = false;
                  setState({ kind: "pending" });
                }
              }
            : undefined
        }
      />
    );
  }

  if (state.kind === "success") {
    return (
      <div className='flex min-h-screen w-full items-center justify-center bg-background p-4'>
        <Card className='w-full max-w-md'>
          <CardHeader className='text-center'>
            <div className='mb-4 flex justify-center'>
              <CheckCircle2 className='h-12 w-12 text-primary' />
            </div>
            <CardTitle className='text-2xl'>Invitation accepted</CardTitle>
            <CardDescription>Redirecting to your new organization…</CardDescription>
          </CardHeader>
        </Card>
      </div>
    );
  }

  return (
    <div className='flex min-h-screen w-full items-center justify-center bg-background p-4'>
      <div className='flex flex-col items-center gap-4 text-center'>
        <Spinner className='size-8 text-primary' />
        <p className='text-muted-foreground text-sm'>Accepting your invitation…</p>
      </div>
    </div>
  );
}

function ErrorCard({
  title,
  description,
  primaryAction
}: {
  title: string;
  description: string;
  primaryAction?: { label: string; onClick: () => void };
}) {
  const navigate = useNavigate();
  return (
    <div className='flex min-h-screen w-full items-center justify-center bg-background p-4'>
      <Card className='w-full max-w-md'>
        <CardHeader className='text-center'>
          <div className='mb-4 flex justify-center'>
            <XCircle className='h-12 w-12 text-destructive' />
          </div>
          <CardTitle className='text-2xl'>{title}</CardTitle>
          <CardDescription>{description}</CardDescription>
        </CardHeader>
        <CardContent className='flex flex-col gap-2'>
          {primaryAction && (
            <Button onClick={primaryAction.onClick} className='w-full'>
              {primaryAction.label}
            </Button>
          )}
          <Button
            variant={primaryAction ? "outline" : "default"}
            onClick={() => navigate(ROUTES.ROOT)}
            className='w-full'
          >
            Back to home
          </Button>
        </CardContent>
      </Card>
    </div>
  );
}
