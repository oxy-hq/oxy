import { createContext } from "react";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { FileType } from "@/utils/fileTypes";

export interface EditorContextValue {
  pathb64: string;
  filePath: string;
  fileType: FileType;
  project: ReturnType<typeof useCurrentProjectBranch>["project"];
  branchName: string;
  isReadOnly: boolean;
  gitEnabled: boolean;
}

export const EditorContext = createContext<EditorContextValue | null>(null);
