import { useEffect, useRef, useState } from "react";

import { join } from "@tauri-apps/api/path";
import { mkdir } from "@tauri-apps/plugin-fs";
import { css } from "styled-system/css";

import Icon from "../../ui/Icon";
import { useFileTree } from "./FileTreeContext";

const newFolderInputStyles = css({
  flex: 1,
  bg: "transparent",
  border: "none",
  outline: "none",
  color: "neutral.text.colorText",
  fontSize: "14px",
  p: 0,
  m: 0,
  "&:focus": {
    outline: "none",
  },
});

const fileItemStyles = css({
  display: "flex",
  alignItems: "center",
  gap: "sm",
  p: "sm",
  cursor: "pointer",
  _hover: {
    bg: "neutral.bg.colorBgHover",
  },
  borderRadius: "borderRadiusSM",
});

const CreateFolderInput = ({
  path,
  setIsCreatingFolder = () => {},
  refreshFolder,
  level = 0,
}: {
  path: string;
  setIsCreatingFolder?: (value: boolean) => void;
  refreshFolder?: () => void;
  level?: number;
}) => {
  const [newFolderName, setNewFolderName] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  const { setFocusedPath } = useFileTree();

  useEffect(() => {
    setNewFolderName("");
    setTimeout(() => {
      inputRef.current?.focus();
    }, 100);
  }, []);

  const handleCreateFolder = async (
    e: React.KeyboardEvent<HTMLInputElement>,
  ) => {
    if (e.key === "Enter" && newFolderName.trim()) {
      try {
        const newFolderPath = await join(path, newFolderName);
        await mkdir(newFolderPath);
        setIsCreatingFolder(false);
        refreshFolder?.();
        setFocusedPath(newFolderPath);
      } catch (error) {
        console.error("Failed to create folder:", error);
      }
    } else if (e.key === "Escape") {
      setIsCreatingFolder(false);
    }
  };

  return (
    <div className={fileItemStyles} style={{ marginLeft: `${level * 28}px` }}>
      <Icon
        asset="folder"
        className={css({ color: "neutral.icon.colorIcon" })}
      />
      <input
        ref={inputRef}
        type="text"
        value={newFolderName}
        onChange={(e) => setNewFolderName(e.target.value)}
        onKeyDown={handleCreateFolder}
        className={newFolderInputStyles}
        placeholder="New Folder"
        onBlur={() => setIsCreatingFolder(false)}
      />
    </div>
  );
};

export default CreateFolderInput;
