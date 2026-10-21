import type React from "react";
import { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Spinner } from "@/components/ui/shadcn/spinner";
import CliAuthCard from "../../components/CliAuthCard";
import DeviceCodeCheck from "../../components/DeviceCodeCheck";

interface Props {
  hostname: string;
  userCode: string;
  email?: string;
  pending: boolean;
  failed: boolean;
  onApprove: () => void;
  onDecline: () => void;
}

/**
 * `oxyc login --no-browser`: the login's own card (`PkceAuthorize`), with the code to type.
 * The login is the widest thing this page approves — the person's organization reach, for a
 * year, though never their staff or partner access (the server mints it without) — so Authorize
 * stays off until the code typed is this request's.
 */
const DeviceLoginApproval: React.FC<Props> = ({
  hostname,
  userCode,
  email,
  pending,
  failed,
  onApprove,
  onDecline
}) => {
  const [codeMatches, setCodeMatches] = useState(false);
  return (
    <CliAuthCard
      status='confirm'
      title={
        <>
          Sign in to oxyc on{" "}
          <span className='break-words' data-testid='cli-auth-hostname'>
            {hostname}
          </span>
          ?
        </>
      }
      description={
        <>
          oxyc on that computer will act as {email ?? "you"}, with what your organization
          memberships reach, for a year. It never carries staff or partner access: sign in with{" "}
          <code>oxyc login</code> on your own computer for that. It replaces any earlier oxyc login
          from a computer of the same name.
        </>
      }
    >
      <div className='flex flex-col gap-3'>
        <DeviceCodeCheck code={userCode} onMatchedChange={setCodeMatches} disabled={pending} />
        {failed && (
          <p
            className='text-center text-destructive text-sm'
            role='alert'
            data-testid='cli-auth-error'
          >
            Couldn't authorize oxyc. Try again, or run <code>oxyc login --no-browser</code> again
            for a new code.
          </p>
        )}
        <div className='flex justify-center gap-2'>
          <Button
            variant='outline'
            onClick={onDecline}
            disabled={pending}
            data-testid='cli-auth-cancel'
          >
            Cancel
          </Button>
          <Button
            onClick={onApprove}
            disabled={pending || !codeMatches}
            data-testid='cli-auth-confirm'
          >
            {pending && <Spinner className='size-4' />}
            Authorize oxyc
          </Button>
        </div>
        <p className='text-center text-muted-foreground text-xs'>
          Continue only if you just ran <code>oxyc login --no-browser</code> on {hostname}.
        </p>
      </div>
    </CliAuthCard>
  );
};

export default DeviceLoginApproval;
