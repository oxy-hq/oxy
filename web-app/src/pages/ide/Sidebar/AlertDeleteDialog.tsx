import { FileTreeModel } from "@/types/file";
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogCancel,
  AlertDialogAction,
} from "@/components/ui/shadcn/alert-dialog";
import useDeleteFolder from "@/hooks/api/files/useDeleteFolder";
import { useLocation, useNavigate } from "react-router-dom";
import useDeleteFile from "@/hooks/api/files/useDeleteFile";

interface AlertDeleteDialogProps {
  fileTree: FileTreeModel;
  visible: boolean;
  setVisible: (visible: boolean) => void;
}

const AlertDeleteDialog = ({
  fileTree,
  visible,
  setVisible,
}: AlertDeleteDialogProps) => {
  const { pathname } = useLocation();
  const deleteFolder = useDeleteFolder();
  const deleteFile = useDeleteFile();
  const navigate = useNavigate();

  const isDir = fileTree.is_dir;

  const handleConfirmDelete = async () => {
    try {
      if (isDir) {
        await deleteFolder.mutateAsync(btoa(fileTree.path));
      } else {
        await deleteFile.mutateAsync(btoa(fileTree.path));
      }
      const currentPath = atob(pathname.split("/").pop() ?? "");
      if (currentPath.startsWith(fileTree.path)) {
        navigate(`/ide`);
      }
    } catch (error) {
      console.error("Failed to delete folder:", error);
    }
    setVisible(false);
  };

  return (
    <AlertDialog open={visible} onOpenChange={setVisible}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Are you sure?</AlertDialogTitle>
          <AlertDialogDescription>
            This action cannot be undone. This will permanently delete the{" "}
            {isDir ? "folder" : "file"}{" "}
            <span className="font-semibold">{fileTree.name}</span> and all its
            contents.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction onClick={handleConfirmDelete}>
            Delete
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
};

export default AlertDeleteDialog;
