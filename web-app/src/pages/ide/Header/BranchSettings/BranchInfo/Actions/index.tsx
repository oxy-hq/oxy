import { Button } from "@/components/ui/shadcn/button";
import { useState } from "react";
import { PullDialog } from "./PullDialog";
import { PushDialog } from "./PushDialog";

const Actions = () => {
  const [pullDialogOpen, setPullDialogOpen] = useState(false);
  const [pushDialogOpen, setPushDialogOpen] = useState(false);

  return (
    <>
      <div className="flex gap-2">
        <Button
          variant="outline"
          className="flex-1"
          onClick={() => setPullDialogOpen(true)}
        >
          Pull changes
        </Button>
        <Button
          variant="outline"
          className="flex-1"
          onClick={() => setPushDialogOpen(true)}
        >
          Push changes
        </Button>
      </div>

      <PullDialog open={pullDialogOpen} onOpenChange={setPullDialogOpen} />

      <PushDialog open={pushDialogOpen} onOpenChange={setPushDialogOpen} />
    </>
  );
};

export default Actions;
