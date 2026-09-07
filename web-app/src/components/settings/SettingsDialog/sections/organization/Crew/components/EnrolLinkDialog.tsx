import { Copy } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle
} from "@/components/ui/shadcn/dialog";
import { Input } from "@/components/ui/shadcn/input";
import type { CreatedKioskDevice } from "@/types/frontline";

/**
 * The one moment the enrol link exists on screen. The server keeps only a hash
 * of the token, so closing this is final — the copy says so plainly rather
 * than guarding the close, because a guard implies there is a way back.
 */
export function EnrolLinkDialog({
  device,
  onClose
}: {
  device: CreatedKioskDevice | null;
  onClose: () => void;
}) {
  const copyLink = async () => {
    if (!device) return;
    try {
      await navigator.clipboard.writeText(device.enrol_url);
      toast.success("Enrol link copied");
    } catch (error) {
      console.error("Failed to copy enrol link:", error);
      toast.error("Couldn't copy — select the link and copy it by hand");
    }
  };

  return (
    <Dialog open={device !== null} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className='sm:max-w-md'>
        <DialogHeader>
          <DialogTitle>Enrol {device?.name}</DialogTitle>
          <DialogDescription>
            Open this link on the tablet. It works once, for 24 hours, and cannot be shown again.
          </DialogDescription>
        </DialogHeader>
        <div className='flex items-center gap-2 pt-1'>
          <Input
            readOnly
            value={device?.enrol_url ?? ""}
            onFocus={(e) => e.currentTarget.select()}
            className='font-mono text-xs'
            aria-label='Enrol link'
            data-testid='settings-crew-enrol-link'
          />
          <Button
            type='button'
            size='sm'
            className='shrink-0 gap-1.5'
            onClick={copyLink}
            data-testid='settings-crew-enrol-link-copy'
          >
            <Copy className='h-4 w-4' />
            Copy link
          </Button>
        </div>
        <div className='flex justify-end'>
          <Button type='button' variant='outline' size='sm' onClick={onClose}>
            Done
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
