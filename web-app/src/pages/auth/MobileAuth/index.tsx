import type React from "react";
import { useEffect, useMemo, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useAuth } from "@/contexts/AuthContext";
import useAuthorizeMobile from "@/hooks/api/mobileAuth/useAuthorizeMobile";
import { apiErrorCode, apiStatus } from "@/libs/apiError";
import MobileAuthCard from "./MobileAuthCard";
import {
  codeRedirectUrl,
  declinedRedirectUrl,
  loginUrl,
  type MobileAuthRequest,
  parseMobileAuthRequest,
  returnToUrl
} from "./mobileAuthRequest";
import { leaveTo, type MobileSession, readMobileSession, returnToApp } from "./mobileSession";
import { startSignIn } from "./startSignIn";

type OkRequest = Extract<MobileAuthRequest, { kind: "ok" }>;
type Step = "checking" | "confirm" | "returning" | "declined" | "refused" | "crew";

const RESTART = "Go back to the Oxygen app and tap Sign in again.";

/** Whether another page is showing this one in a frame (a cross-origin check that throws counts). */
const isFramed = (): boolean => {
  try {
    return window.top !== window.self;
  } catch {
    return true;
  }
};

/**
 * `/mobile-auth` — the browser side of signing in the Oxygen mobile app.
 *
 * The app opens this page in the phone's sign-in sheet with `?code_challenge&redirect_uri&state`
 * (PKCE), and optionally `&provider=google|github|okta|email`. Nothing is authorized, and nothing
 * leaves the page, until the person reads which account the app will act as and taps Continue.
 * Then the session is traded for a single-use code, and only that code goes back to the app, at
 * its `oxygen://` redirect URI; the app exchanges it, with a verifier only it holds, for its own
 * session.
 *
 * Not signed in: through login first, coming back to this exact request; straight to the
 * provider's own sign-in when the app passed the one the person picked (`provider`), so they don't
 * pick it a second time on the login page. Framed: refused, since a frame lets the outer page cover
 * the card and steer a tap onto Continue.
 */
const MobileAuth: React.FC = () => {
  const [params] = useSearchParams();
  const request = useMemo(() => parseMobileAuthRequest(params), [params]);

  if (isFramed()) {
    return (
      <MobileAuthCard
        status='error'
        title='Open this page in its own tab'
        description='This page approves a sign-in, so it does not work inside another page. Nothing was sent.'
      />
    );
  }
  if (request.kind === "invalid") {
    return (
      <MobileAuthCard
        status='error'
        title="This sign-in link doesn't work"
        description={request.reason}
      />
    );
  }
  return <MobileApproval request={request} />;
};

const MobileApproval: React.FC<{ request: OkRequest }> = ({ request }) => {
  const [step, setStep] = useState<Step>("checking");
  const [session, setSession] = useState<MobileSession | null>(null);
  const authorize = useAuthorizeMobile();
  const { authConfig } = useAuth();
  const returnTo = returnToUrl(window.location.origin, request);
  const loginHref = loginUrl(returnTo);

  useEffect(() => {
    let cancelled = false;
    readMobileSession()
      .catch(() => null)
      .then((found) => {
        if (cancelled) return;
        if (!found) return startSignIn(request.provider, authConfig, returnTo);
        setSession(found);
        setStep("confirm");
      })
      .catch(() => {
        if (!cancelled) leaveTo(loginHref);
      });
    return () => {
      cancelled = true;
    };
  }, [request.provider, authConfig, returnTo, loginHref]);

  const confirm = () =>
    authorize.mutate(
      {
        code_challenge: request.codeChallenge,
        code_challenge_method: "S256",
        redirect_uri: request.redirectUri
      },
      {
        onSuccess: ({ code }) => {
          setStep("returning");
          returnToApp(codeRedirectUrl(request, code));
        },
        onError: (error) => {
          // The session lapsed while the page sat open: sign in again and come back here.
          if (apiStatus(error) === 401) leaveTo(loginHref);
          else if (apiErrorCode(error) === "invalid_redirect_uri") setStep("refused");
          // A crew member's kiosk session: their sign-in stays on the store's device.
          else if (apiErrorCode(error) === "frontline_not_supported") setStep("crew");
        }
      }
    );

  const decline = () => {
    setStep("declined");
    returnToApp(declinedRedirectUrl(request));
  };

  switch (step) {
    case "checking":
      return (
        <MobileAuthCard
          status='working'
          title='Checking your session…'
          description='Making sure you are signed in before the Oxygen app asks for access.'
        />
      );
    case "returning":
      return (
        <MobileAuthCard
          status='done'
          title='Signed in'
          description='Returning to the Oxygen app. You can close this page if it stays open.'
        />
      );
    case "declined":
      return (
        <MobileAuthCard
          status='error'
          title='Sign-in cancelled'
          description='Nothing was shared with the Oxygen app. Returning to it now.'
        />
      );
    case "refused":
      return (
        <MobileAuthCard
          status='error'
          title="This sign-in link doesn't work"
          description={`Oxygen doesn't recognise where the app asked to return. ${RESTART}`}
        />
      );
    case "crew":
      return (
        <MobileAuthCard
          status='error'
          title="Crew accounts can't use the Oxygen app"
          description="Crew sign-in works only on a store's enrolled device. Nothing was shared with the app; sign in on the store's tablet instead."
        />
      );
    case "confirm":
      return (
        <Confirm
          session={session}
          pending={authorize.isPending}
          failed={authorize.isError && apiStatus(authorize.error) !== 401}
          onConfirm={confirm}
          onDecline={decline}
        />
      );
  }
};

interface ConfirmProps {
  session: MobileSession | null;
  pending: boolean;
  failed: boolean;
  onConfirm: () => void;
  onDecline: () => void;
}

const Confirm: React.FC<ConfirmProps> = ({ session, pending, failed, onConfirm, onDecline }) => {
  const who = session?.name ?? session?.email ?? "you";
  const showEmail = Boolean(session?.name && session?.email);
  return (
    <MobileAuthCard
      status='confirm'
      initial={who === "you" ? undefined : who.charAt(0).toUpperCase()}
      title={
        <>
          Sign in to the Oxygen app as{" "}
          <span className='break-words' data-testid='mobile-auth-account'>
            {who}
          </span>
          ?
        </>
      }
      description={
        <>
          {showEmail && (
            <span className='t-small mb-2 block text-muted-foreground'>{session?.email}</span>
          )}
          The app will act as you, with everything you can reach in Oxygen.
        </>
      }
    >
      <div className='flex flex-col gap-3'>
        {failed && (
          <p className='t-small text-destructive' role='alert' data-testid='mobile-auth-error'>
            Couldn't sign in the app. Try again, or tap Sign in in the app for a new link.
          </p>
        )}
        <div className='grid grid-cols-2 gap-2'>
          <Button
            variant='outline'
            onClick={onDecline}
            disabled={pending}
            data-testid='mobile-auth-cancel'
          >
            Cancel
          </Button>
          <Button onClick={onConfirm} disabled={pending} data-testid='mobile-auth-continue'>
            {pending && <Spinner className='size-4' />}
            Continue
          </Button>
        </div>
        <p className='t-small text-muted-foreground'>
          Continue only if you just tapped Sign in in the Oxygen app.
        </p>
      </div>
    </MobileAuthCard>
  );
};

export default MobileAuth;
