import { useQuery } from "@tanstack/react-query";
import { getLanguageFromFileName } from "@/components/FileEditor/constants";
import { BaseMonacoEditor } from "@/components/MonacoEditor";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { encodeBase64 } from "@/libs/encoding";
import { FileService } from "@/services/api";
import type { FileStatus } from "@/types/file";
import { MergeConflictEditor } from "./MergeConflictEditor";

interface Props {
  file: FileStatus;
  splitView: boolean;
  onConflictResolved?: () => void;
}

function RegularFileDiff({ file, splitView }: Pick<Props, "file" | "splitView">) {
  const { project, branchName } = useCurrentProjectBranch();
  const pathb64 = encodeBase64(file.path);
  const isAdded = file.status === "A";
  const isDeleted = file.status === "D";
  const language = getLanguageFromFileName(file.path);

  const { data: originalContent = "" } = useQuery({
    queryKey: ["file-from-git", project.id, branchName, file.path],
    queryFn: () => FileService.getFileFromGit(project.id, pathb64, branchName),
    enabled: !isAdded,
    retry: false
  });

  const { data: currentContent = "" } = useQuery({
    queryKey: ["file-current", project.id, branchName, file.path],
    queryFn: () => FileService.getFile(project.id, pathb64, branchName),
    enabled: !isDeleted,
    retry: false
  });

  return (
    <BaseMonacoEditor
      value={currentContent}
      original={isAdded ? "" : originalContent}
      diffMode
      splitView={splitView}
      language={language}
      path={file.path}
      height='100%'
      options={{
        readOnly: true,
        minimap: { enabled: false },
        scrollBeyondLastLine: false,
        fontSize: 12,
        lineNumbers: "on",
        wordWrap: "on",
        wrappingStrategy: "advanced"
      }}
    />
  );
}

export function FileDiff({ file, splitView, onConflictResolved }: Props) {
  if (file.status === "U") {
    return <MergeConflictEditor file={file} onResolved={onConflictResolved ?? (() => {})} />;
  }
  return <RegularFileDiff file={file} splitView={splitView} />;
}
