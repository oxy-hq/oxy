import { useState, useEffect, useCallback } from "react";
import { FileState } from "@/components/FileEditor";

interface UseFileContentManagerProps {
  initialContent?: string;
  onValueChange?: (value: string) => void;
  onFileStateChange: (state: FileState) => void;
  readOnly?: boolean;
}

export const useFileContentManager = ({
  initialContent = "",
  onValueChange,
  onFileStateChange,
  readOnly = false,
}: UseFileContentManagerProps) => {
  const [content, setContent] = useState(initialContent);
  const [isDirty, setIsDirty] = useState(false);

  useEffect(() => {
    setContent(initialContent);
    setIsDirty(false);
    onFileStateChange("saved");
  }, [initialContent, onFileStateChange]);

  useEffect(() => {
    onValueChange?.(content);
  }, [content, onValueChange]);

  const handleContentChange = useCallback(
    (newContent: string) => {
      if (readOnly) return;

      setContent(newContent);
      const isContentDirty = newContent !== initialContent;
      setIsDirty(isContentDirty);
      onFileStateChange(isContentDirty ? "modified" : "saved");
    },
    [readOnly, initialContent, onFileStateChange],
  );

  const resetContent = useCallback(() => {
    setContent(initialContent);
    setIsDirty(false);
    onFileStateChange("saved");
  }, [initialContent, onFileStateChange]);

  return {
    content,
    isDirty,
    handleContentChange,
    resetContent,
  };
};
