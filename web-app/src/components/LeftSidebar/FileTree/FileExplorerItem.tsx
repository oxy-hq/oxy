import { useCallback, useState } from "react";

import {
  create,
  DirEntry,
  mkdir,
  readDir,
  remove,
} from "@tauri-apps/plugin-fs";
import { css, cx } from "styled-system/css";

import Text from "@/components/ui/Typography/Text";

import Icon from "../../ui/Icon";
import FileExplorerView from "./FileExplorerView";
import { useFileTree } from "./FileTreeContext";
import MoreDropdown from "./MoreDropdown";
import RenameInput from "./RenameInput";
import { useNavigate } from "react-router-dom";

const styles = {
  fileItem: css({
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    gap: "sm",
    px: "sm",
    cursor: "pointer",
    _hover: { bg: "neutral.bg.colorBgHover" },
    borderRadius: "borderRadiusSM",
    "&[data-focused='true']": { bg: "neutral.bg.colorBgActive" },
    "&[data-menu-open=true]": { bgColor: "surface.tertiary" },
    maxH: "36px",
  }),

  moreButton: css({
    display: "none!",
    '.group[data-menu-open="true"] &': { display: "flex!" },
    _groupHover: { display: "flex!" },
  }),

  contentWrapper: css({
    display: "flex",
    justifyContent: "space-between",
    alignItems: "center",
    flex: 1,
    minWidth: 0,
  }),

  nameSection: css({
    display: "flex",
    alignItems: "center",
    gap: "sm",
    flex: 1,
    minWidth: 0,
  }),

  fileName: css({
    color: "neutral.text.colorText",
    whiteSpace: "nowrap",
    overflow: "hidden",
    textOverflow: "ellipsis",
  }),

  icon: css({
    color: "neutral.icon.colorIcon",
  }),

  itemContent: css({
    display: "flex",
    alignItems: "center",
    gap: "sm",
    py: "sm",
    flex: 1,
    minW: 0,
  }),
};

interface FileExplorerItemProps {
  entry: DirEntry;
  level?: number;
  path: string;
  refreshFolder?: () => void;
}

const FileExplorerItem = ({
  entry,
  level = 0,
  path,
  refreshFolder,
}: FileExplorerItemProps) => {
  const [isExpanded, setIsExpanded] = useState(false);
  const [dirChildren, setDirChildren] = useState<DirEntry[]>([]);
  const [isMoreMenuOpen, setIsMoreMenuOpen] = useState(false);
  const [isRenaming, setIsRenaming] = useState(false);
  const navigate = useNavigate();

  const isDirectory = entry.isDirectory;
  const { focusedPath, setFocusedPath } = useFileTree();
  const fullPath = `${path}/${entry.name}`;

  const fetchDirChildren = useCallback(async () => {
    if (!isDirectory) return;
    const dirEntries = await readDir(fullPath);
    setDirChildren(dirEntries);
  }, [fullPath, isDirectory]);

  const toggleExpanded = useCallback(
    async (value: boolean) => {
      if (!isExpanded && value) {
        await fetchDirChildren();
      }
      setIsExpanded(value);
    },
    [isExpanded, fetchDirChildren],
  );

  const handleClick = async (e: React.MouseEvent) => {
    e.stopPropagation();
    if (entry.isFile && entry.name.endsWith(".workflow.yml")) {
      // navigate to workflow page
      const filePathBase64 = btoa(`${path}/${entry.name}`);
      navigate(`/workflow/${filePathBase64}`);
      return;
    }

    if (entry.isFile && entry.name.endsWith(".agent.yml")) {
      // navigate to agent page
      const filePathBase64 = btoa(`${path}/${entry.name}`);
      navigate(`/agent/${filePathBase64}`);
      return;
    }

    if (entry.isFile) {
      const filePathBase64 = btoa(`${path}/${entry.name}`);
      navigate(`/file/${filePathBase64}`);
      return;
    }

    if (isDirectory) {
      setFocusedPath(fullPath);
      await toggleExpanded(!isExpanded);
    }
  };

  const handleDelete = async () => {
    await remove(fullPath, { recursive: true });
    refreshFolder?.();
  };

  const handleDuplicate = async () => {
    const newFileName = `${entry.name}_copy`;
    const newFilePath = `${path}/${newFileName}`;

    try {
      if (isDirectory) {
        await mkdir(newFilePath);
      } else {
        await create(newFilePath);
      }
      refreshFolder?.();
    } catch (error) {
      console.error(`Failed to duplicate ${entry.name}:`, error);
    }
  };

  const renderFileIcon = () => (
    <Icon
      // eslint-disable-next-line sonarjs/no-nested-conditional
      asset={isDirectory ? (isExpanded ? "folder_open" : "folder") : "file"}
      className={styles.icon}
    />
  );

  return (
    <li>
      <div
        className={cx(styles.fileItem, "group")}
        style={{ marginLeft: `${level * 28}px` }}
        data-focused={focusedPath === fullPath}
        data-menu-open={isMoreMenuOpen}
      >
        <div onClick={handleClick} className={styles.itemContent}>
          {isDirectory && (
            <Icon
              asset={isExpanded ? "chevron_down" : "chevron_right"}
              className={styles.icon}
            />
          )}

          <div className={styles.nameSection}>
            {renderFileIcon()}
            {isRenaming ? (
              <RenameInput
                initialName={entry.name}
                path={path}
                setIsRenaming={setIsRenaming}
                refreshFolder={refreshFolder}
              />
            ) : (
              <Text variant="bodyBaseRegular" className={styles.fileName}>
                {entry.name}
              </Text>
            )}
          </div>
        </div>

        {!isRenaming && (
          <MoreDropdown
            onDelete={handleDelete}
            onRename={() => setIsRenaming(true)}
            onDuplicate={handleDuplicate}
            isOpen={isMoreMenuOpen}
            onOpenChange={setIsMoreMenuOpen}
            className={styles.moreButton}
          />
        )}
      </div>

      {isDirectory && (
        <FileExplorerView
          entries={dirChildren}
          level={level + 1}
          path={fullPath}
          show={isExpanded}
          refreshFolder={fetchDirChildren}
          onExpand={() => toggleExpanded(true)}
        />
      )}
    </li>
  );
};

export default FileExplorerItem;
