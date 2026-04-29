import { Code, Eye } from "lucide-react";
import { useState } from "react";
import { useFileEditorContext } from "@/components/FileEditor/useFileEditorContext";
import Markdown from "@/components/Markdown";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import EditorPageWrapper from "../components/EditorPageWrapper";
import { useEditorContext } from "../contexts/useEditorContext";

enum ViewMode {
  Split = "split",
  Preview = "preview"
}

const MarkdownPreview = () => {
  const {
    state: { content }
  } = useFileEditorContext();

  return (
    <div className='h-full overflow-auto p-6'>
      <Markdown>{content}</Markdown>
    </div>
  );
};

const ModeSwitcher = ({
  viewMode,
  onViewModeChange
}: {
  viewMode: ViewMode;
  onViewModeChange: (mode: ViewMode) => void;
}) => (
  <Tabs
    value={viewMode}
    onValueChange={(value: string) => {
      if (Object.values(ViewMode).includes(value as ViewMode)) {
        onViewModeChange(value as ViewMode);
      }
    }}
  >
    <TabsList>
      <TabsTrigger value={ViewMode.Split} aria-label='Split view'>
        <Code />
      </TabsTrigger>
      <TabsTrigger value={ViewMode.Preview} aria-label='Preview only'>
        <Eye />
      </TabsTrigger>
    </TabsList>
  </Tabs>
);

const MarkdownEditor = () => {
  const { pathb64, gitEnabled } = useEditorContext();
  const [viewMode, setViewMode] = useState<ViewMode>(ViewMode.Split);

  return (
    <EditorPageWrapper
      pathb64={pathb64}
      git={gitEnabled}
      defaultDirection='horizontal'
      headerPrefixAction={<ModeSwitcher viewMode={viewMode} onViewModeChange={setViewMode} />}
      preview={<MarkdownPreview />}
      previewOnly={viewMode === ViewMode.Preview}
    />
  );
};

export default MarkdownEditor;
