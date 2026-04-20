import { Key } from "lucide-react";
import GithubIcon from "@/components/ui/GithubIcon";
import { Button } from "@/components/ui/shadcn/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle
} from "@/components/ui/shadcn/dialog";

interface Props {
  open: boolean;
  onClose: () => void;
  onSelectApp: () => void;
  onSelectPAT: () => void;
}

export default function AddNamespaceMenu({ open, onClose, onSelectApp, onSelectPAT }: Props) {
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className='sm:max-w-md'>
        <DialogHeader>
          <DialogTitle>Connect GitHub</DialogTitle>
          <DialogDescription>Choose how to connect your GitHub account.</DialogDescription>
        </DialogHeader>

        <div className='flex flex-col gap-2'>
          <Button className='w-full gap-2' onClick={onSelectApp}>
            <GithubIcon className='h-4 w-4' />
            Via GitHub App
          </Button>
          <Button variant='outline' className='w-full gap-2' onClick={onSelectPAT}>
            <Key className='h-4 w-4' />
            Via personal access token
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
