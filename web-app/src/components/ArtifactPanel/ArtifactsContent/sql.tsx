import { Editor } from "@monaco-editor/react";
import { AlertCircle, Loader2 } from "lucide-react";
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

      {!!artifact.content.value.error && (
        <div className='mx-3 my-2 flex max-h-32 flex-1 items-start gap-2 overflow-y-auto rounded-md border border-red-500/30 bg-red-500/10 px-3 py-2 text-red-400 text-sm'>
          <AlertCircle className='mt-0.5 h-4 w-4 shrink-0' />
          <span className='whitespace-pre-wrap'>{artifact.content.value.error}</span>
        </div>
      )}

      {(!artifact.content.value.error || artifact.content.value.result_file) && (
        <div className='flex-1 overflow-auto'>
          <SqlResultsTable
            result={artifact.content.value.result}
            resultFile={artifact.content.value.result_file}
          />
        </div>
      )}
    </div>
  );
};

export default SqlArtifactPanel;
