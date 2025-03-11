import { useEffect, useState } from "react";

import { css } from "styled-system/css";

import CreateFileInput from "./CreateFiIeInput";
import CreateFolderInput from "./CreateFolderInput";
import FileExplorerItem from "./FileExplorerItem";

const fileTreeStyles = css({
  display: "flex",
  flexDirection: "column",
  gap: "xs",
  px: "paddingContentHorizontalLG",
  py: "sm",
  "&[data-show='false']": {
    display: "none",
  },
});

interface FileExplorerViewProps {
  entries: DirEntry[];
  level?: number;
  path: string;
  show?: boolean;
  refreshFolder?: () => void;
  onExpand?: () => void;
}

const sortEntries = (entries: DirEntry[]) =>
  entries.sort((a, b) => {
    if (a.isDirectory !== b.isDirectory) {
      return a.isDirectory ? -1 : 1;
    }
    return a.name.localeCompare(b.name);
  });

const FileExplorerView = ({
  entries,
  level = 0,
  path,
  show = false,
  refreshFolder,
  onExpand,
}: FileExplorerViewProps) => {
  const [isCreatingFolderHere, setIsCreatingFolderHere] = useState(false);
  const [isCreatingFileHere, setIsCreatingFileHere] = useState(false);

  useEffect(() => {
    const handleCreationEvent = (
      event: CustomEvent<{ path: string }>,
      setCreating: (value: boolean) => void,
    ) => {
      const isCreatingAtRoot = !event.detail.path && level === 0;
      if (isCreatingAtRoot || event.detail.path === path) {
        onExpand?.();
        setCreating(true);
      }
    };

    const handleCreateFolder = (e: CustomEvent<{ path: string }>) =>
      handleCreationEvent(e, setIsCreatingFolderHere);

    const handleCreateFile = (e: CustomEvent<{ path: string }>) =>
      handleCreationEvent(e, setIsCreatingFileHere);

    window.addEventListener(
      "createFolderInPath",
      handleCreateFolder as EventListener,
    );
    window.addEventListener(
      "createFileInPath",
      handleCreateFile as EventListener,
    );

    return () => {
      window.removeEventListener(
        "createFolderInPath",
        handleCreateFolder as EventListener,
      );
      window.removeEventListener(
        "createFileInPath",
        handleCreateFile as EventListener,
      );
    };
  }, [level, onExpand, path]);

  return (
    <ul data-show={show} className={fileTreeStyles}>
      {isCreatingFolderHere && (
        <CreateFolderInput
          level={level}
          path={path}
          setIsCreatingFolder={setIsCreatingFolderHere}
          refreshFolder={refreshFolder}
        />
      )}
      {isCreatingFileHere && (
        <CreateFileInput
          level={level}
          path={path}
          setIsCreatingFile={setIsCreatingFileHere}
          refreshFolder={refreshFolder}
        />
      )}
      {sortEntries(entries).map((entry) => (
        <FileExplorerItem
          key={`${path}/${entry.name}`}
          entry={entry}
          refreshFolder={refreshFolder}
          path={path}
          level={level}
        />
      ))}
    </ul>
  );
};

export default FileExplorerView;
