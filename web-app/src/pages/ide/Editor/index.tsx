import { useParams } from "react-router-dom";
import { useState } from "react";
import FileEditor, { FileState } from "@/components/FileEditor";
import WorkflowPreview from "@/pages/workflow/WorkflowPreview";
import AgentPreview from "./AgentPreview";
import SqlEditorPage from "./SqlEditor";
import Header from "./Header";

// eslint-disable-next-line sonarjs/pseudo-random
const randomKey = () => Math.random().toString(36).substring(2, 15);

const Editor = ({ pathb64 }: { pathb64: string }) => {
  const filePath = atob(pathb64 ?? "");
  const isWorkflow = filePath.endsWith(".workflow.yml");
  const isAgent = filePath.endsWith(".agent.yml");
  const isSql = filePath.endsWith(".sql");
  const [fileState, setFileState] = useState<FileState>("saved");
  const [previewKey, setPreviewKey] = useState<string>(randomKey());

  if (isSql) {
    return <SqlEditorPage />;
  }

  return (
    <div className="flex h-full md:flex-row flex-col">
      <div className="flex-1 md:w-[50%] w-full md:h-full h-[50%] flex flex-col bg-[#1e1e1e]">
        <Header filePath={filePath} fileState={fileState} />
        <FileEditor
          fileState={fileState}
          pathb64={pathb64 ?? ""}
          onFileStateChange={setFileState}
          onSaved={() => {
            setPreviewKey(randomKey());
          }}
        />
      </div>

      {isWorkflow && (
        <div className="flex-1">
          <WorkflowPreview key={previewKey} pathb64={pathb64 ?? ""} />
        </div>
      )}
      {isAgent && (
        <div className="flex-1">
          <AgentPreview key={previewKey} agentPathb64={pathb64 ?? ""} />
        </div>
      )}
    </div>
  );
};

const EditorPage = () => {
  const { pathb64 } = useParams();
  return <Editor key={pathb64 ?? ""} pathb64={pathb64 ?? ""} />;
};

export default EditorPage;
