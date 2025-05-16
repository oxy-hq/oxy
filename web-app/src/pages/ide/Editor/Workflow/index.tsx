import { useState } from "react";
import EditorPageWrapper from "../components/EditorPageWrapper";
import { randomKey } from "@/libs/utils/string";
import { WorkflowPreview } from "@/pages/workflow/WorkflowPreview";

const WorkflowEditor = ({ pathb64 }: { pathb64: string }) => {
  const [previewKey, setPreviewKey] = useState<string>(randomKey());
  return (
    <EditorPageWrapper
      pathb64={pathb64}
      pageContentClassName="md:flex-row flex-col"
      editorClassName="md:w-1/2 w-full h-1/2 md:h-full"
      onSaved={() => {
        setPreviewKey(randomKey());
      }}
      preview={
        <div className="flex-1">
          <WorkflowPreview key={previewKey} pathb64={pathb64 ?? ""} />
        </div>
      }
    />
  );
};
export default WorkflowEditor;
