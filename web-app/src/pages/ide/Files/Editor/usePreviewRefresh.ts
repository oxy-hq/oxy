import { useState, useCallback } from "react";
import { randomKey } from "@/libs/utils/string";

export const usePreviewRefresh = () => {
  const [previewKey, setPreviewKey] = useState<string>(randomKey());

  const refreshPreview = useCallback(() => {
    setPreviewKey(randomKey());
  }, []);

  return {
    previewKey,
    refreshPreview,
  };
};
