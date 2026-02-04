import { Editor } from "@monaco-editor/react";
import { Loader2 } from "lucide-react";
import SqlResultsTable from "@/components/sql/SqlResultsTable";
import type { OmniQueryArtifact } from "@/types/artifact";

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
    <div className='flex h-full flex-col'>
      {/* Omni Query JSON Section */}
      <div className='border-b p-4'>
        <h4 className='mb-2 font-medium text-sm'>Omni Query</h4>
        <Editor
          height='200px'
          width='100%'
          theme='vs-dark'
          defaultValue={JSON.stringify(getCleanOmniObject(artifact.content.value), null, 2)}
          language='json'
          value={JSON.stringify(getCleanOmniObject(artifact.content.value), null, 2)}
          loading={<Loader2 className='h-4 w-4 animate-[spin_0.2s_linear_infinite] text-[white]' />}
          options={{
            readOnly: true,
            scrollBeyondLastLine: false,
            formatOnPaste: true,
            formatOnType: true,
            automaticLayout: true,
            minimap: { enabled: false },
            wordWrap: "on"
          }}
        />
      </div>

      <h4 className='mb-2 font-medium text-sm'>Generated SQL</h4>
      {/* Generated SQL Section */}
      <div className='flex-1'>
        <Editor
          height='100%'
          width='100%'
          theme='vs-dark'
          defaultValue={artifact.content.value.sql}
          language='sql'
          value={artifact.content.value.sql}
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

      {/* Results Section */}
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

export default OmniQueryArtifactPanel;
