import { Mail } from "lucide-react";
import { useState } from "react";
import { useForm } from "react-hook-form";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { useAuth } from "@/contexts/AuthContext";
import { useRequestMagicLink } from "@/hooks/auth/useMagicLink";
import { cn } from "@/libs/shadcn/utils";
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
  const { mutateAsync: requestMagicLink, isPending } = useRequestMagicLink();

  const {
    register,
    handleSubmit,
    formState: { errors }
  } = useForm<MagicLinkFormData>();

  const onSubmit = async (data: MagicLinkFormData) => {
    try {
      await requestMagicLink({ email: data.email });
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
      await requestMagicLink({ email: submittedEmail });
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
            {isPending ? "Resending…" : "Didn't receive it? Resend"}
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
        {errors.email && <p className='text-red-500 text-sm'>{errors.email.message}</p>}
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

const LoginForm = () => {
  const { authConfig } = useAuth();

  const hasOAuth = authConfig.google || authConfig.okta;
  const hasMagicLink = authConfig.magic_link;

  return (
    <div className={cn("flex flex-col gap-6")}>
      <div className='flex flex-col items-center gap-2 text-center'>
        <h1 className='font-bold text-2xl'>Welcome back</h1>
        <p className='text-muted-foreground text-sm'>Sign in to your account to continue</p>
      </div>

      <div className='flex flex-col gap-4'>
        {hasMagicLink && <MagicLinkSection />}

        {hasOAuth && hasMagicLink && <Divider label='or' />}

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
      </div>
    </div>
  );
};

export default LoginForm;
