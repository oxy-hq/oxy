import type React from "react";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import MintPage from "../../components/MintPage";
import { cleanUserCode } from "../deviceRequest";

/**
 * Where a person types the code oxyc printed, when they came here without the link that carries
 * it. Nothing is asked of the server until the code could be one.
 */
const DeviceCodeEntry: React.FC<{ invalid?: boolean }> = ({ invalid = false }) => {
  const navigate = useNavigate();
  const [typed, setTyped] = useState("");
  const [problem, setProblem] = useState<string | null>(
    invalid ? "That link carries no code oxyc prints. Type the code from your terminal." : null
  );

  const submit = (event: React.FormEvent) => {
    event.preventDefault();
    const code = cleanUserCode(typed);
    if (!code) {
      setProblem("A code is eight letters, like BCDF-GHJK.");
      return;
    }
    navigate(`/cli-auth/device?code=${encodeURIComponent(code)}`);
  };

  return (
    <MintPage
      status='confirm'
      title='Enter the code from your terminal'
      lead='oxyc printed it when it asked for a token with --no-browser.'
    >
      <form className='mt-6 flex flex-col gap-3' onSubmit={submit}>
        <Input
          aria-label='Code'
          autoComplete='off'
          autoCapitalize='characters'
          className='max-w-60 font-mono uppercase tracking-widest'
          placeholder='BCDF-GHJK'
          value={typed}
          onChange={(event) => setTyped(event.target.value)}
          data-testid='cli-auth-device-code-input'
        />
        {problem && (
          <p className='text-destructive text-sm' role='alert'>
            {problem}
          </p>
        )}
        <div>
          <Button type='submit' data-testid='cli-auth-device-code-continue'>
            Continue
          </Button>
        </div>
      </form>
    </MintPage>
  );
};

export default DeviceCodeEntry;
