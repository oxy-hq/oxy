import React from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Plus, Loader2 } from "lucide-react";

interface FormActionsProps {
  onCancel: () => void;
  isCreating: boolean;
  isValid: boolean;
}

export const FormActions: React.FC<FormActionsProps> = ({
  onCancel,
  isCreating,
  isValid,
}) => {
  return (
    <div className="flex gap-3 justify-end">
      <Button
        type="button"
        variant="outline"
        onClick={onCancel}
        disabled={isCreating}
      >
        Cancel
      </Button>
      <Button type="submit" disabled={isCreating || !isValid}>
        {isCreating ? (
          <Loader2 className="h-4 w-4 animate-spin" />
        ) : (
          <Plus className="h-4 w-4" />
        )}
        Create
      </Button>
    </div>
  );
};
