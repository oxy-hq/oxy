import { useLocation, useNavigate } from "react-router-dom";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle
} from "@/components/ui/shadcn/alert-dialog";
import useDeleteFile from "@/hooks/api/files/useDeleteFile";
import useDeleteFolder from "@/hooks/api/files/useDeleteFolder";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import ROUTES from "@/libs/utils/routes";
import type { FileTreeModel } from "@/types/file";

interface AlertDeleteDialogProps {
  fileTree: FileTreeModel;
  visible: boolean;
  setVisible: (visible: boolean) => void;
}

const AlertDeleteDialog = ({ fileTree, visible, setVisible }: AlertDeleteDialogProps) => {
  const { pathname } = useLocation();
  const deleteFolder = useDeleteFolder();
  const deleteFile = useDeleteFile();
  const navigate = useNavigate();
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

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
        const ideUri = ROUTES.PROJECT(projectId).IDE.ROOT;
        navigate(ideUri);
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
            {isDir ? "folder" : "file"} <span className='font-semibold'>{fileTree.name}</span> and
            all its contents.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction onClick={handleConfirmDelete}>Delete</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
};

export default AlertDeleteDialog;
