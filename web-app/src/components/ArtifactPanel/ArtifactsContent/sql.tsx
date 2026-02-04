import { Editor } from "@monaco-editor/react";
import { Loader2 } from "lucide-react";
import SqlResultsTable from "@/components/sql/SqlResultsTable";
import type { SqlArtifact } from "@/types/artifact";

type Props = {
  artifact: SqlArtifact;
};

const SqlArtifactPanel = ({ artifact }: Props) => {
  return (
    <div className='flex h-full flex-col'>
      <div className='flex-1'>
        <Editor
          height='100%'
          width='100%'
          theme='vs-dark'
          defaultValue={artifact.content.value.sql_query}
          language='sql'
          value={artifact.content.value.sql_query}
          loading={<Loader2 className='h-4 w-4 animate-[spin_0.2s_linear_infinite] text-[white]' />}
          options={{
            readOnly: true,
            scrollBeyondLastLine: true,
            formatOnPaste: true,
            formatOnType: true,
            automaticLayout: true
          }}
        />
      </div>

      <div className='flex-1 overflow-auto'>
        {(artifact.content.value.result || artifact.content.value.result_file) && (
          <SqlResultsTable
            result={artifact.content.value.result}
            resultFile={artifact.content.value.result_file}
          />
        )}
      </div>
    </div>
  );
};

export default SqlArtifactPanel;
