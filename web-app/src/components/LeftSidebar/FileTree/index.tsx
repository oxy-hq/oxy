import { useCallback, useEffect, useState } from "react";

import { css } from "styled-system/css";

import Button from "@/components/ui/Button";
import Icon from "@/components/ui/Icon";
import Text from "@/components/ui/Typography/Text";
import useProjectPath from "@/stores/useProjectPath";

import FileExplorerView from "./FileExplorerView";
import { useFileTree } from "./FileTreeContext";

const wrapperStyles = css({
  display: "flex",
  flexDirection: "column",
});

const headerStyles = css({
  display: "flex",
  flexDirection: "row",
  alignItems: "center",
  justifyContent: "space-between",
  py: "paddingContentHorizontalLG",
  color: "neutral.text.colorTextSecondary",
  gap: "sm",
});

const FileTree = ({
  openOpenAIAPIKeyModal,
}: {
  openOpenAIAPIKeyModal: () => void;
}) => {
  const { projectPath } = useProjectPath();
  const [dirChildren, setDirChildren] = useState<DirEntry[]>([]);
  const { focusedPath } = useFileTree();

  const fetchDirChildren = useCallback(async () => {
    const dirHandle = await window.showDirectoryPicker();
    const entries = [];
    for await (const entry of dirHandle.values()) {
      entries.push(entry);
    }
    setDirChildren(entries);
  }, [projectPath]);

  useEffect(() => {
    fetchDirChildren();
  }, [fetchDirChildren, projectPath]);

  const projectName = projectPath.split("/").pop();

  const handleCreateFolder = () => {
    const event = new CustomEvent("createFolderInPath", {
      detail: { path: focusedPath },
    });
    window.dispatchEvent(event);
  };

  const handleCreateFile = () => {
    const event = new CustomEvent("createFileInPath", {
      detail: { path: focusedPath },
    });
    window.dispatchEvent(event);
  };

  return (
    <div className={wrapperStyles}>
      <div className={headerStyles}>
        <Text variant="label14Medium" color="secondary">
          {projectName}
        </Text>
        <div className={css({ display: "flex", gap: "sm" })}>
          <Button content="icon" variant="ghost" onClick={handleCreateFolder}>
            <Icon asset="folder_add" />
          </Button>
          <Button content="icon" variant="ghost" onClick={handleCreateFile}>
            <Icon asset="file_add" />
          </Button>
          <Button
            content="icon"
            variant="ghost"
            onClick={openOpenAIAPIKeyModal}
          >
            <Icon asset="settings" />
          </Button>
        </div>
      </div>
      <FileExplorerView
        entries={dirChildren}
        show={true}
        path={projectPath}
        refreshFolder={fetchDirChildren}
      />
    </div>
  );
};

export default FileTree;
