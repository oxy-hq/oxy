import Editor from "@monaco-editor/react";
import SqlResultsTable from "@/components/sql/SqlResultsTable";
import ErrorAlert from "@/components/ui/ErrorAlert";
import { Spinner } from "@/components/ui/shadcn/spinner";
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
          loading={<Spinner />}
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
        <ErrorAlert
          className='mx-3 my-2 max-h-32 overflow-y-auto'
          message={artifact.content.value.error}
        />
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
