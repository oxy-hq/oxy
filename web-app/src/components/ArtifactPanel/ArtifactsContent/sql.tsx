import { SqlArtifact } from "@/types/artifact";
import Results from "@/pages/ide/Editor/Sql/Results";
import { Editor } from "@monaco-editor/react";
import { Loader2 } from "lucide-react";

type Props = {
  artifact: SqlArtifact;
};

const SqlArtifactPanel = ({ artifact }: Props) => {
  return (
    <div className="flex flex-col h-full">
      <div className="flex-1">
        <Editor
          height="100%"
          width="100%"
          theme="vs-dark"
          defaultValue={artifact.content.value.sql_query}
          language="sql"
          value={artifact.content.value.sql_query}
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

      <div className="flex-1 overflow-auto">
        {(artifact.content.value.result ||
          artifact.content.value.result_file) && (
          <Results
            result={artifact.content.value.result}
            resultFile={artifact.content.value.result_file}
          />
        )}
      </div>
    </div>
  );
};

export default SqlArtifactPanel;
