import { useEffect, useRef, useState } from "react";
import { Volume } from "memfs";
import { css } from "styled-system/css";
import Icon from "../../ui/Icon";

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

const CreateFileInput = ({
  path,
  setIsCreatingFile = () => {},
  refreshFolder,
  level = 0,
}: {
  path: string;
  setIsCreatingFile?: (value: boolean) => void;
  refreshFolder?: () => void;
  level?: number;
}) => {
  const [newFileName, setNewFileName] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    setNewFileName("");
    setTimeout(() => {
      inputRef.current?.focus();
    }, 100);
  }, []);

  const handleCreateFile = async (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter" && newFileName.trim()) {
      try {
        const vol = new Volume();
        const newFilePath = `${path}/${newFileName}`;
        vol.writeFileSync(newFilePath, "");
        setIsCreatingFile(false);
        refreshFolder?.();
      } catch (error) {
        console.error("Failed to create folder:", error);
      }
    } else if (e.key === "Escape") {
      setIsCreatingFile(false);
    }
  };

  return (
    <div className={fileItemStyles} style={{ marginLeft: `${level * 28}px` }}>
      <Icon asset="file" className={css({ color: "neutral.icon.colorIcon" })} />
      <input
        ref={inputRef}
        type="text"
        value={newFileName}
        onChange={(e) => setNewFileName(e.target.value)}
        onKeyDown={handleCreateFile}
        className={newFolderInputStyles}
        placeholder="New File"
        onBlur={() => setIsCreatingFile(false)}
      />
    </div>
  );
};

export default CreateFileInput;
