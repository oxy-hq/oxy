import { useEffect, useRef, useState } from "react";

import { css } from "styled-system/css";

import { useFileTree } from "./FileTreeContext";

interface RenameInputProps {
  initialName: string;
  path: string;
  setIsRenaming: (value: boolean) => void;
  refreshFolder?: () => void;
}

const inputStyles = css({
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

const RenameInput = ({
  initialName,
  path,
  setIsRenaming,
  refreshFolder,
}: RenameInputProps) => {
  const [newName, setNewName] = useState(initialName);
  const inputRef = useRef<HTMLInputElement>(null);
  const { setFocusedPath } = useFileTree();

  useEffect(() => {
    const focusInput = () => {
      if (!inputRef.current) return;

      inputRef.current.focus();
      const extensionIndex = initialName.lastIndexOf(".");
      const selectionEnd =
        extensionIndex > 0 ? extensionIndex : initialName.length;
      inputRef.current.setSelectionRange(0, selectionEnd);
    };

    const timeoutId = setTimeout(focusInput, 100);
    return () => clearTimeout(timeoutId);
  }, [initialName]);

  const isValidNewName = (name: string): boolean => {
    return (
      name.trim() !== "" &&
      name !== initialName &&
      !name.includes("/") &&
      !name.includes("\\")
    );
  };

  const handleRename = async (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      if (!isValidNewName(newName)) {
        return;
      }

      try {
        const newPath = `${path}/${newName}`;
        await window.showDirectoryPicker().then(async (dirHandle) => {
          const fileHandle = await dirHandle.getFileHandle(initialName);
          const file = await fileHandle.getFile();
          const newFileHandle = await dirHandle.getFileHandle(newName, {
            create: true,
          });
          const writable = await newFileHandle.createWritable();
          await writable.write(await file.arrayBuffer());
          await writable.close();
          await dirHandle.removeEntry(initialName);
          return true;
        });
        refreshFolder?.();
        setIsRenaming(false);
        setFocusedPath(newPath);
      } catch (error) {
        console.error("Failed to rename:", error);
      }
    } else if (e.key === "Escape") {
      setIsRenaming(false);
    }
  };

  return (
    <input
      ref={inputRef}
      type="text"
      value={newName}
      onChange={(e) => setNewName(e.target.value)}
      onKeyDown={handleRename}
      className={inputStyles}
      onBlur={() => setIsRenaming(false)}
    />
  );
};

export default RenameInput;
