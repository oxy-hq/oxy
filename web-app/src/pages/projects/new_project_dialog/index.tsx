import { Button } from "@/components/ui/shadcn/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/shadcn/dialog";
import { Plus } from "lucide-react";
import { NewProjectForm } from "./NewProjectForm";

export default function NewProjectDialog({
  onOpenChange,
  isOpen,
}: {
  onOpenChange: (open: boolean) => void;
  isOpen: boolean;
}) {
  return (
    <Dialog open={isOpen} onOpenChange={onOpenChange}>
      <DialogTrigger asChild>
        <Button size={"sm"}>
          <Plus className="h-4 w-4 mr-2" />
          New project
        </Button>
      </DialogTrigger>
      <DialogContent className="!max-w-2xl max-h-[90vh] overflow-y-auto gap-10">
        <DialogHeader>
          <DialogTitle>Create New Project</DialogTitle>
        </DialogHeader>
        <NewProjectForm onClose={() => onOpenChange(false)} />
      </DialogContent>
    </Dialog>
  );
}
