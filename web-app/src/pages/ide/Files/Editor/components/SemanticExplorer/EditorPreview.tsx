import type { JSX } from "react";
import { useEffect, useState } from "react";
import { useFilesContext } from "../../../FilesContext";
import { FilesSubViewMode } from "../../../FilesSidebar/constants";
import EditorPageWrapper from "../EditorPageWrapper";
import ModeSwitcher from "./ModeSwitcher";
import { ViewMode } from "./types";

interface EditorPreviewProps {
  pathb64: string;
  isReadOnly: boolean;
  explorer: JSX.Element;
}

const EditorPreview = ({ pathb64, isReadOnly, explorer }: EditorPreviewProps) => {
  const { filesSubViewMode } = useFilesContext();

  const defaultViewMode =
    filesSubViewMode === FilesSubViewMode.OBJECTS ? ViewMode.Explorer : ViewMode.Editor;

  const [viewMode, setViewMode] = useState<ViewMode>(defaultViewMode);

  useEffect(() => {
    setViewMode(defaultViewMode);
  }, [defaultViewMode]);

  return (
    <EditorPageWrapper
      headerPrefixAction={<ModeSwitcher viewMode={viewMode} onViewModeChange={setViewMode} />}
      pathb64={pathb64}
      readOnly={isReadOnly}
      defaultDirection='horizontal'
      preview={explorer}
      previewOnly={viewMode === ViewMode.Explorer}
    />
  );
};

export default EditorPreview;
