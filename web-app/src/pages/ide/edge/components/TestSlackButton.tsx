import { Bell, Loader2 } from "lucide-react";
import type React from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import useTestSlackAlert from "@/hooks/api/cameras/useTestSlackAlert";

/**
 * Operator-facing "fire a no-op Slack alert" button so the
 * notification wiring can be validated without a real camera
 * degradation. Lives next to "Add device" in the EdgeBoxesTable
 * header — same context where an operator is already verifying
 * their fleet setup.
 *
 * Backend `delivered: false` is treated as a normal state (no
 * Slack install / no default channel), not an error.
 */
const TestSlackButton: React.FC<{ workspaceId: string | undefined }> = ({ workspaceId }) => {
  const mut = useTestSlackAlert(workspaceId);

  const handleClick = async () => {
    try {
      const { delivered } = await mut.mutateAsync();
      if (delivered) {
        toast.success("Slack alert sent — check your default channel.");
      } else {
        toast.info("No Slack installation or default channel configured.");
      }
    } catch (e) {
      toast.error(e instanceof Error ? e.message : "Failed to send test alert.");
    }
  };

  return (
    <Button size='sm' variant='outline' onClick={handleClick} disabled={mut.isPending}>
      {mut.isPending ? (
        <Loader2 className='h-3.5 w-3.5 animate-spin' />
      ) : (
        <Bell className='h-3.5 w-3.5' />
      )}
      Test Slack
    </Button>
  );
};

export default TestSlackButton;
