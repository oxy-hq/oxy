import { Mail } from "lucide-react";
import { useState } from "react";
import { useForm } from "react-hook-form";
import { Link, useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import { FieldError } from "@/components/ui/shadcn/field";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useAuth } from "@/contexts/AuthContext";
import {
  returnToPointsAtCustomApp,
  useFrontlineRoster,
  useKioskDevice
} from "@/hooks/auth/useFrontline";
import { useRequestMagicLink } from "@/hooks/auth/useMagicLink";
import { cn } from "@/libs/shadcn/utils";
import ROUTES from "@/libs/utils/routes";
import CrewSignIn, { CrewSignInHint } from "./CrewSignIn";
import LoginWithGitHubButton from "./LoginWithGitHubButton";
import LoginWithGoogleButton from "./LoginWithGoogleButton";
import LoginWithOktaButton from "./LoginWithOktaButton";

type MagicLinkFormData = {
  email: string;
};

const isRateLimited = (error: unknown) =>
  (error as { response?: { status?: number } })?.response?.status === 429;

const getRateLimitMessage = (error: unknown) =>
  (error as { response?: { data?: { message?: string } } })?.response?.data?.message ??
  "Too many sign-in attempts. Please try again later.";

type View = "form" | "sent";

const MagicLinkSection = () => {
  const [view, setView] = useState<View>("form");
  const [submittedEmail, setSubmittedEmail] = useState("");
  const [searchParams] = useSearchParams();
  // Forwarded into the magic-link request. The server allowlists the value
  // before embedding it into the email; the verify-callback page validates
  // again before performing the redirect.
  const returnTo = searchParams.get("return_to") ?? undefined;
  const { mutateAsync: requestMagicLink, isPending } = useRequestMagicLink();

  const {
    register,
    handleSubmit,
    formState: { errors }
  } = useForm<MagicLinkFormData>();

  const onSubmit = async (data: MagicLinkFormData) => {
    try {
      await requestMagicLink({ email: data.email, return_to: returnTo });
      setSubmittedEmail(data.email);
      setView("sent");
    } catch (error) {
      if (isRateLimited(error)) {
        toast.error(getRateLimitMessage(error));
      } else {
        toast.error("Something went wrong. Please try again.");
      }
    }
  };

  const handleResend = async () => {
    try {
      await requestMagicLink({ email: submittedEmail, return_to: returnTo });
      toast.success("Sign-in link resent.");
    } catch (error) {
      if (isRateLimited(error)) {
        toast.error(getRateLimitMessage(error));
      } else {
        toast.error("Something went wrong. Please try again.");
      }
    }
  };

  if (view === "sent") {
    return (
      <div className='flex flex-col items-center gap-4 text-center'>
        <div className='flex h-14 w-14 items-center justify-center rounded-full bg-primary/10'>
          <Mail className='h-7 w-7 text-primary' />
        </div>
        <div className='flex flex-col gap-1'>
          <h2 className='font-semibold text-lg'>Check your inbox</h2>
          <p className='text-muted-foreground text-sm'>
            We sent a sign-in link to{" "}
            <span className='font-medium text-foreground'>{submittedEmail}</span>. It expires in 15
            minutes.
          </p>
        </div>
        <div className='flex flex-col gap-2 text-sm'>
          <button
            type='button'
            onClick={handleResend}
            disabled={isPending}
            className='text-primary underline-offset-4 hover:underline disabled:opacity-50'
          >
            {isPending ? <Spinner /> : "Didn't receive it? Resend"}
          </button>
          <button
            type='button'
            onClick={() => setView("form")}
            className='text-muted-foreground underline-offset-4 hover:underline'
          >
            Use a different email
          </button>
        </div>
      </div>
    );
  }

  return (
    <form onSubmit={handleSubmit(onSubmit)} className='flex flex-col gap-3'>
      <div className='grid gap-2'>
        <Label htmlFor='magic-email'>Email</Label>
        <Input
          id='magic-email'
          type='email'
          placeholder='you@example.com'
          {...register("email", {
            required: "Email is required",
            pattern: {
              value: /^[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}$/i,
              message: "Invalid email address"
            }
          })}
          disabled={isPending}
        />
        {errors.email && <FieldError>{errors.email.message}</FieldError>}
      </div>
      <Button type='submit' className='w-full' disabled={isPending}>
        {isPending ? "Sending link…" : "Continue with email"}
      </Button>
    </form>
  );
};

const Divider = ({ label }: { label: string }) => (
  <div className='relative text-center text-sm after:absolute after:inset-0 after:top-1/2 after:z-0 after:flex after:items-center after:border-border after:border-t'>
    <span className='relative z-10 bg-background px-2 text-muted-foreground'>{label}</span>
  </div>
);

/**
 * Only rendered when the server reports the bypass reachable *by this caller*
 * (`authConfig.dev_login` — see the field's docstring; an inferred allow-list
 * is loopback-only, so this is absent off-box). It exists so an agent driving
 * the browser has something to click — the login page is where automation
 * lands, and every other button here leads off to a provider or an inbox.
 */
const DevSignInSection = () => (
  <Link to={ROUTES.AUTH.DEV_LOGIN} className='w-full' data-testid='login-dev-signin'>
    <Button type='button' variant='outline' className='w-full'>
      Dev sign-in (no password)
    </Button>
  </Link>
);

const ACCOUNT_COPY = {
  title: "Welcome back",
  subtitle: "Sign in to your account to continue"
};

/** A kiosk speaks to whoever is standing at it, not to an account holder. */
const kioskCopy = (tapToPick: boolean) => ({
  title: "Who's on shift?",
  subtitle: tapToPick ? "Tap your name and enter your PIN" : "Enter your ID and PIN"
});

const LoginForm = () => {
  const { authConfig } = useAuth();
  const [searchParams] = useSearchParams();
  // Crew sign-in's first-choice destination (validated server-side before any
  // redirect). The magic-link section reads the same param on its own.
  const returnTo = searchParams.get("return_to") ?? undefined;
  const { data: device } = useKioskDevice();
  const kiosk = device?.bound ? device : undefined;
  const { data: staff = [], isLoading: isRosterLoading } = useFrontlineRoster(kiosk?.org);

  const hasOAuth = Boolean(authConfig.google || authConfig.okta || authConfig.github);
  const hasMagicLink = Boolean(authConfig.magic_link);
  const hasAccountSignIn = hasMagicLink || hasOAuth;
  // While the roster is still loading, assume the common case (there is one)
  // so the subtitle doesn't flip mid-read.
  const copy = kiosk ? kioskCopy(isRosterLoading || staff.length > 0) : ACCOUNT_COPY;

  return (
    <div className={cn("flex flex-col gap-6")}>
      <div className='flex flex-col items-center gap-2 text-center'>
        <h1 className='font-bold text-2xl'>{copy.title}</h1>
        <p className='text-muted-foreground text-sm'>{copy.subtitle}</p>
      </div>

      <div className='flex flex-col gap-4'>
        {kiosk && (
          <CrewSignIn
            device={kiosk}
            staff={staff}
            isRosterLoading={isRosterLoading}
            returnTo={returnTo}
          />
        )}
        {kiosk && hasAccountSignIn && <Divider label='or' />}

        {hasMagicLink && <MagicLinkSection />}

        {hasOAuth && hasMagicLink && <Divider label='or' />}

        {authConfig.github && (
          <LoginWithGitHubButton disabled={false} clientId={authConfig.github.client_id} />
        )}
        {authConfig.google && (
          <LoginWithGoogleButton disabled={false} clientId={authConfig.google.client_id} />
        )}
        {authConfig.okta && (
          <LoginWithOktaButton
            disabled={false}
            clientId={authConfig.okta.client_id}
            domain={authConfig.okta.domain}
          />
        )}

        {authConfig.dev_login && (
          <>
            <Divider label='dev only' />
            <DevSignInSection />
          </>
        )}

        {/* Only once the probe has answered "not a kiosk" — never during the
            probe, or a real kiosk would flash the hint before its board. */}
        {device && !device.bound && returnToPointsAtCustomApp(returnTo) && <CrewSignInHint />}
      </div>
    </div>
  );
};

export default LoginForm;
