import { Loader2 } from "lucide-react";
import type React from "react";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle
} from "@/components/ui/shadcn/dialog";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import useBindExistingDevice from "@/hooks/api/cameras/useBindExistingDevice";
import type { EdgeBoxSummary } from "@/services/api/cameras";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";

/**
 * Retroactive claim flow for a legacy bearer-auth edge box.
 *
 * Pre-condition: the box has already updated to the new factory
 * image, which means the worker prints a `device_id` (UUID v7)
 * to its startup log. The operator reads it from
 * `docker logs oxy-worker | grep edge.boot` and pastes it here.
 *
 * Backend rejects (a) devices that haven't factory-enrolled and
 * (b) edge boxes already linked to another device, both with
 * clear error messages — we surface those via toast rather than
 * pre-validating in the UI.
 */
type Props = {
  box: EdgeBoxSummary | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

// RFC 4122 — accept any UUID format including v7 (what the
// factory image generates). We match leniently and let the
// backend do the authoritative check.
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

const LinkToFleetDialog: React.FC<Props> = ({ box, open, onOpenChange }) => {
  const { workspace } = useCurrentWorkspace();
  const bind = useBindExistingDevice(workspace?.id);
  const [deviceId, setDeviceId] = useState("");

  useEffect(() => {
    if (!open) setDeviceId("");
  }, [open]);

  const handleSubmit = async () => {
    const trimmed = deviceId.trim().toLowerCase();
    if (!UUID_RE.test(trimmed)) {
      toast.error("Enter a valid device UUID (the worker prints it to its log on boot).");
      return;
    }
    if (!box) return;
    try {
      await bind.mutateAsync({ deviceId: trimmed, edgeBoxId: box.id });
      toast.success("Device linked. It will appear in the Fleet tab.");
      onOpenChange(false);
    } catch (e) {
      const msg = e instanceof Error ? e.message : "Failed to link device.";
      toast.error(msg);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Link this box to the fleet</DialogTitle>
          <DialogDescription>
            {box ? (
              <>
                Attach a known <code>device_id</code> to{" "}
                <span className='font-mono'>{box.hardware_model}</span>. Read the UUID off the
                worker startup log (<code>docker logs oxy-worker | grep edge.boot</code>) on the box
                and paste it below.
              </>
            ) : (
              "Attach a known device_id to this edge box."
            )}
          </DialogDescription>
        </DialogHeader>

        <div className='flex flex-col gap-1'>
          <Label htmlFor='device-id'>Device ID</Label>
          <Input
            id='device-id'
            value={deviceId}
            onChange={(e) => setDeviceId(e.target.value)}
            placeholder='e.g. 0192c83a-7d62-7000-b3f4-1a8c2e3d4f5b'
            autoFocus
            disabled={bind.isPending}
            className='font-mono'
          />
          <span className='text-muted-foreground text-xs'>
            The device must have already updated to the new factory image — otherwise it hasn't
            self-enrolled yet and the link will be rejected.
          </span>
        </div>

        <DialogFooter>
          <Button variant='ghost' onClick={() => onOpenChange(false)} disabled={bind.isPending}>
            Cancel
          </Button>
          <Button onClick={handleSubmit} disabled={bind.isPending}>
            {bind.isPending && <Loader2 className='mr-1.5 h-4 w-4 animate-spin' />}
            Link
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};

export default LinkToFleetDialog;
