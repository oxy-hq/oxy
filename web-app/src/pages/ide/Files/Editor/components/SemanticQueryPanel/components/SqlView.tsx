import type { ReactNode } from "react";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneDark } from "react-syntax-highlighter/dist/esm/styles/prism";

interface SqlViewProps {
  generatedSql: string;
  sqlError: string | null;
  loading?: boolean;
  loadingIndicator?: ReactNode;
}

const SqlView = ({ generatedSql, sqlError, loading, loadingIndicator }: SqlViewProps) => {
  return (
    <div className='customScrollbar h-full overflow-auto p-4'>
      {(() => {
        if (sqlError) {
          return (
            <div className='whitespace-pre-wrap rounded bg-destructive/10 p-4 font-mono text-destructive text-xs'>
              {sqlError}
            </div>
          );
        }
        if (generatedSql) {
          return (
            <SyntaxHighlighter
              language='sql'
              style={oneDark}
              customStyle={{ margin: 0, borderRadius: "0.5rem" }}
              className='font-mono text-xs'
            >
              {generatedSql}
            </SyntaxHighlighter>
          );
        }
        if (loading) {
          return (
            <div className='flex h-full items-center justify-center'>
              {loadingIndicator ?? (
                <span className='text-muted-foreground text-sm'>Building SQL...</span>
              )}
            </div>
          );
        }
        return (
          <div className='flex h-full items-center justify-center text-muted-foreground text-sm'>
            Run a query to see the generated SQL
          </div>
        );
      })()}
    </div>
  );
};

export default SqlView;
