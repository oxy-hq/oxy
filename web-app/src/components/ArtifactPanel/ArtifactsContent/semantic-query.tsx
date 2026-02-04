import { Editor } from "@monaco-editor/react";
import { Loader2 } from "lucide-react";
import SqlResultsTable from "@/components/sql/SqlResultsTable";
import type { SemanticQueryArtifact } from "@/types/artifact";

type Props = {
  artifact: SemanticQueryArtifact;
};

const getCleanSemanticObject = (value: SemanticQueryArtifact["content"]["value"]) => {
  // Only filter out execution-related fields, keep all semantic parameters
  /* eslint-disable @typescript-eslint/no-unused-vars */
  /* eslint-disable sonarjs/no-unused-vars */
  const {
    error: _error,
    sql_generation_error: _sql_generation_error,
    validation_error: _validation_error,
    result: _,
    sql_query: _sql_query,
    is_result_truncated: _is_result_truncated,
    database: _database,
    ...semanticParams
  } = value;
  /* eslint-enable @typescript-eslint/no-unused-vars */
  /* eslint-enable sonarjs/no-unused-vars */
  return semanticParams;
};

const SemanticQueryArtifactPanel = ({ artifact }: Props) => {
  const renderError = () => {
    if (artifact.content.value.validation_error) {
      return (
        <div className='m-4 rounded-md border border-red-500/50 bg-red-900/20 p-4'>
          <h4 className='mb-2 font-medium text-red-400 text-sm'>Validation Error</h4>
          <pre className='whitespace-pre-wrap text-red-300 text-sm'>
            {artifact.content.value.validation_error}
          </pre>
        </div>
      );
    }

    if (artifact.content.value.sql_generation_error) {
      return (
        <div className='m-4 rounded-md border border-red-500/50 bg-red-900/20 p-4'>
          <h4 className='mb-2 font-medium text-red-400 text-sm'>SQL Generation Error</h4>
          <pre className='whitespace-pre-wrap text-red-300 text-sm'>
            {artifact.content.value.sql_generation_error}
          </pre>
        </div>
      );
    }

    if (artifact.content.value.error) {
      return (
        <div className='m-4 rounded-md border border-red-500/50 bg-red-900/20 p-4'>
          <h4 className='mb-2 font-medium text-red-400 text-sm'>Execution Error</h4>
          <pre className='whitespace-pre-wrap text-red-300 text-sm'>
            {artifact.content.value.error}
          </pre>
        </div>
      );
    }

    return null;
  };

  return (
    <div className='flex h-full flex-col'>
      {/* Semantic Query JSON Section */}
      <div className='border-b p-4'>
        <h4 className='mb-2 font-medium text-sm'>Semantic Query</h4>
        <Editor
          height='200px'
          width='100%'
          theme='vs-dark'
          defaultValue={JSON.stringify(getCleanSemanticObject(artifact.content.value), null, 2)}
          language='json'
          value={JSON.stringify(getCleanSemanticObject(artifact.content.value), null, 2)}
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

      {/* Results Section */}
      <div className='flex-1 overflow-auto'>
        {(artifact.content.value.result || artifact.content.value.result_file) && (
          <SqlResultsTable
            result={artifact.content.value.result}
            resultFile={artifact.content.value.result_file}
          />
        )}
      </div>
      <div className='flex-1 overflow-auto'>{renderError()}</div>
    </div>
  );
};

export default SemanticQueryArtifactPanel;
