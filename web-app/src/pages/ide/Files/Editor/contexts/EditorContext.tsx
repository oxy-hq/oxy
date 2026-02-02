import { ReactNode } from "react";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { decodeFilePath, detectFileType } from "@/utils/fileTypes";
import { EditorContext, EditorContextValue } from "./EditorContextTypes";

interface EditorProviderProps {
  children: ReactNode;
  pathb64: string;
}

export const EditorProvider = ({ children, pathb64 }: EditorProviderProps) => {
  const { project, branchName, isReadOnly, gitEnabled } =
    useCurrentProjectBranch();
  const filePath = decodeFilePath(pathb64);
  const fileType = detectFileType(filePath);

  const value: EditorContextValue = {
    pathb64,
    filePath,
    fileType,
    project,
    branchName,
    isReadOnly: !!isReadOnly,
    gitEnabled: !!gitEnabled,
  };

  return (
    <EditorContext.Provider value={value}>{children}</EditorContext.Provider>
  );
};
