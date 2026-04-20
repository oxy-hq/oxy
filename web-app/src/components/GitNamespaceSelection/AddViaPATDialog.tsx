import { Loader2 } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle
} from "@/components/ui/shadcn/dialog";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { useCreatePATNamespace } from "@/hooks/api/github";

interface Props {
  orgId: string;
  open: boolean;
  onClose: () => void;
  onConnected: (namespaceId: string) => void;
}

export default function AddViaPATDialog({ orgId, open, onClose, onConnected }: Props) {
  const [token, setToken] = useState("");
  const [error, setError] = useState<string | null>(null);
  const { mutate: createPAT, isPending } = useCreatePATNamespace();

  const handleSubmit = () => {
    if (!token.trim()) return;
    setError(null);
    createPAT(
      { orgId, token: token.trim() },
      {
        onSuccess: (ns) => {
          setToken("");
          onConnected(ns.id);
        },
        onError: (e) => setError(e.message || "Invalid token — needs 'repo' scope.")
      }
    );
  };

  const handleClose = () => {
    setToken("");
    setError(null);
    onClose();
  };

  return (
    <Dialog open={open} onOpenChange={(o) => !o && handleClose()}>
      <DialogContent className='sm:max-w-md'>
        <DialogHeader>
          <DialogTitle>Connect via Personal Access Token</DialogTitle>
          <DialogDescription>
            Generate a token at{" "}
            <a
              href='https://github.com/settings/tokens/new?scopes=repo&description=Oxy'
              target='_blank'
              rel='noreferrer'
              className='text-primary underline-offset-4 hover:underline'
            >
              github.com/settings/tokens
            </a>{" "}
            with the <code className='rounded bg-muted px-1 text-xs'>repo</code> scope.
          </DialogDescription>
        </DialogHeader>

        <div className='space-y-4'>
          <div className='space-y-1.5'>
            <Label htmlFor='pat-input'>Token</Label>
            <Input
              id='pat-input'
              type='password'
              placeholder='ghp_… or github_pat_…'
              value={token}
              onChange={(e) => setToken(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && handleSubmit()}
              autoComplete='off'
            />
            {error && <p className='text-destructive text-xs'>{error}</p>}
          </div>

          <Button className='w-full' onClick={handleSubmit} disabled={!token.trim() || isPending}>
            {isPending && <Loader2 className='mr-2 h-4 w-4 animate-spin' />}
            Connect
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
