import { OmniQueryArtifact } from "@/types/artifact";
import Results from "@/pages/ide/Editor/Sql/Results";
import { Editor } from "@monaco-editor/react";
import { Loader2 } from "lucide-react";

type Props = {
  artifact: OmniQueryArtifact;
};

const getCleanOmniObject = (value: OmniQueryArtifact["content"]["value"]) => {
  // Only filter out execution-related fields, keep all omni parameters
  /* eslint-disable @typescript-eslint/no-unused-vars */
  /* eslint-disable sonarjs/no-unused-vars */
  const {
    result: _,
    sql: _sql_query,
    is_result_truncated: _is_result_truncated,
    ...omniParams
  } = value;
  /* eslint-enable @typescript-eslint/no-unused-vars */
  /* eslint-enable sonarjs/no-unused-vars */
  return omniParams;
};

const OmniQueryArtifactPanel = ({ artifact }: Props) => {
  return (
    <div className="flex flex-col h-full">
      {/* Omni Query JSON Section */}
      <div className="p-4 border-b">
        <h4 className="font-medium text-sm mb-2">Omni Query</h4>
        <Editor
          height="200px"
          width="100%"
          theme="vs-dark"
          defaultValue={JSON.stringify(
            getCleanOmniObject(artifact.content.value),
            null,
            2,
          )}
          language="json"
          value={JSON.stringify(
            getCleanOmniObject(artifact.content.value),
            null,
            2,
          )}
          loading={
            <Loader2 className="w-4 h-4 animate-[spin_0.2s_linear_infinite] text-[white]" />
          }
          options={{
            readOnly: true,
            scrollBeyondLastLine: false,
            formatOnPaste: true,
            formatOnType: true,
            automaticLayout: true,
            minimap: { enabled: false },
            wordWrap: "on",
          }}
        />
      </div>

      <h4 className="font-medium text-sm mb-2">Generated SQL</h4>
      {/* Generated SQL Section */}
      <div className="flex-1">
        <Editor
          height="100%"
          width="100%"
          theme="vs-dark"
          defaultValue={artifact.content.value.sql}
          language="sql"
          value={artifact.content.value.sql}
          loading={
            <Loader2 className="w-4 h-4 animate-[spin_0.2s_linear_infinite] text-[white]" />
          }
          options={{
            readOnly: true,
            scrollBeyondLastLine: true,
            formatOnPaste: true,
            formatOnType: true,
            automaticLayout: true,
          }}
        />
      </div>

      {/* Results Section */}
      <div className="flex-1 overflow-auto">
        {!!artifact.content.value.result && (
          <Results result={artifact.content.value.result} />
        )}
      </div>
    </div>
  );
};

export default OmniQueryArtifactPanel;
