import { useBlocker } from "react-router-dom";
import { useEffect, useState } from "react";
import { FileState } from "@/components/FileEditor";

export function useNavigationBlock(fileState: FileState) {
  const [unsavedChangesDialogOpen, setUnsavedChangesDialogOpen] =
    useState(false);

  const blocker = useBlocker(() => {
    if (fileState === "modified") {
      setUnsavedChangesDialogOpen(true);
      return true;
    }
    return false;
  });

  useEffect(() => {
    const handleBeforeUnload = (e: BeforeUnloadEvent) => {
      if (fileState === "modified") {
        e.preventDefault();
        e.returnValue = "";
      }
    };

    window.addEventListener("beforeunload", handleBeforeUnload);
    return () => {
      window.removeEventListener("beforeunload", handleBeforeUnload);
    };
  }, [fileState]);

  return {
    unsavedChangesDialogOpen,
    setUnsavedChangesDialogOpen,
    blocker,
  };
}
