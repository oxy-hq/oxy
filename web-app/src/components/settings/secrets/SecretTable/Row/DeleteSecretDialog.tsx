import React from "react";
import { Button } from "@/components/ui/shadcn/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/shadcn/dialog";
import { AlertTriangle } from "lucide-react";
import { Secret } from "@/types/secret";

interface DeleteSecretDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  secret: Secret | null;
  onConfirm: () => void;
}

export const DeleteSecretDialog: React.FC<DeleteSecretDialogProps> = ({
  open,
  onOpenChange,
  secret,
  onConfirm,
}) => {
  if (!secret) {
    return null;
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[425px]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <AlertTriangle className="h-5 w-5 text-destructive" />
            Delete Secret
          </DialogTitle>
          <DialogDescription>
            This action cannot be undone. This will permanently delete the
            secret.
          </DialogDescription>
        </DialogHeader>

        <div className="py-4">
          <div className="bg-muted p-4 rounded-lg">
            <p className="text-sm font-medium">Deleting secret:</p>
            <p className="text-sm text-muted-foreground mt-1">{secret.name}</p>
            {secret.description && (
              <>
                <p className="text-sm font-medium mt-3">Description:</p>
                <p className="text-sm text-muted-foreground mt-1">
                  {secret.description}
                </p>
              </>
            )}
          </div>

          <div className="mt-4 p-4 bg-orange-50 border border-orange-200 rounded-lg dark:bg-orange-950/20 dark:border-orange-800/30">
            <div className="flex items-start gap-2">
              <AlertTriangle className="h-4 w-4 text-orange-600 mt-0.5 flex-shrink-0 dark:text-orange-400" />
              <div className="text-sm">
                <p className="font-medium text-orange-800 dark:text-orange-300">
                  Warning
                </p>
                <p className="text-orange-700 mt-1 dark:text-orange-200">
                  Any configurations using this secret will lose access and may
                  stop functioning properly. Make sure to update all references
                  before deleting.
                </p>
              </div>
            </div>
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button variant="destructive" onClick={onConfirm}>
            Delete Secret
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};
