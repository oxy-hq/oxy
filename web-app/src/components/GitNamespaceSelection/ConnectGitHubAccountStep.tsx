import { Loader2 } from "lucide-react";
import GithubIcon from "@/components/ui/GithubIcon";
import { Button } from "@/components/ui/shadcn/button";
import { useConnectGitHubAccount } from "@/hooks/api/github";

interface Props {
  orgId: string;
}

export default function ConnectGitHubAccountStep({ orgId }: Props) {
  const { mutate: connect, isPending } = useConnectGitHubAccount();

  return (
    <div className='space-y-2'>
      <Button className='w-full gap-2' onClick={() => connect({ orgId })} disabled={isPending}>
        {isPending ? (
          <Loader2 className='h-4 w-4 animate-spin' />
        ) : (
          <GithubIcon className='h-4 w-4' />
        )}
        {isPending ? "Waiting for GitHub…" : "Connect GitHub"}
      </Button>
    </div>
  );
}
