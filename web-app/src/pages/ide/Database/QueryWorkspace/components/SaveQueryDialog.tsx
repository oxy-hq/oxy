import { useState, useEffect } from "react";
import { toast } from "sonner";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/shadcn/dialog";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { Alert, AlertDescription } from "@/components/ui/shadcn/alert";
import { AlertCircle, Loader2 } from "lucide-react";
import useDatabaseClient, { QueryTab } from "@/stores/useDatabaseClient";
import { FileService } from "@/services/api";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { useQueryClient } from "@tanstack/react-query";
import queryKeys from "@/hooks/api/queryKey";

interface SaveQueryDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  tab: QueryTab | undefined;
}

export default function SaveQueryDialog({
  open,
  onOpenChange,
  tab,
}: SaveQueryDialogProps) {
  const { updateTab } = useDatabaseClient();
  const { project, branchName } = useCurrentProjectBranch();
  const queryClient = useQueryClient();

  const [fileName, setFileName] = useState("");
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Initialize form when dialog opens
  useEffect(() => {
    if (open && tab) {
      // Default to the tab name, but ensure it ends with .sql
      let name = tab.savedPath
        ? tab.savedPath.split("/").pop() || tab.name
        : tab.name;
      if (!name.endsWith(".sql")) {
        name = name.replace(/\.[^/.]+$/, "") + ".sql";
      }
      setFileName(name);
      setError(null);
    }
  }, [open, tab]);

  const handleSave = async () => {
    if (!tab || !fileName.trim()) {
      setError("File name is required");
      return;
    }

    const finalFileName = fileName.endsWith(".sql")
      ? fileName
      : `${fileName}.sql`;
    const filePath = finalFileName; // Save in root directory
    const pathb64 = btoa(filePath);

    setIsSaving(true);
    setError(null);

    try {
      // Use saveFile which will create or update the file
      await FileService.saveFile(project.id, pathb64, tab.content, branchName);

      updateTab(tab.id, {
        name: finalFileName,
        savedPath: filePath,
        isDirty: false,
      });

      queryClient.removeQueries({
        queryKey: queryKeys.file.tree(project.id, branchName),
      });

      toast.success(`Saved to ${filePath}`);
      onOpenChange(false);
    } catch (err) {
      console.error("Failed to save query:", err);
      setError(err instanceof Error ? err.message : "Failed to save query");
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>Save Query</DialogTitle>
          <DialogDescription>
            Save this query to the root directory
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          {error && (
            <Alert variant="destructive">
              <AlertCircle className="h-4 w-4" />
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          )}

          <div className="space-y-2">
            <Label htmlFor="filename">File Name</Label>
            <Input
              id="filename"
              value={fileName}
              onChange={(e) => setFileName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && fileName.trim()) {
                  handleSave();
                }
              }}
              placeholder="my_query.sql"
            />
            <p className="text-xs text-muted-foreground">
              File will be saved in the root directory
            </p>
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button onClick={handleSave} disabled={isSaving || !fileName.trim()}>
            {isSaving ? (
              <>
                <Loader2 className="h-4 w-4 animate-spin mr-1" />
                Saving...
              </>
            ) : (
              "Save"
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
