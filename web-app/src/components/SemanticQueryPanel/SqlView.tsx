import type { ReactNode } from "react";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import ErrorAlert from "@/components/ui/ErrorAlert";
import usePrismTheme from "@/hooks/usePrismTheme";

interface SqlViewProps {
  generatedSql: string;
  sqlError: string | null;
  loading?: boolean;
  loadingIndicator?: ReactNode;
}

const SqlView = ({ generatedSql, sqlError, loading, loadingIndicator }: SqlViewProps) => {
  const prismTheme = usePrismTheme();
  return (
    <div className='h-full overflow-auto p-4'>
      {(() => {
        if (sqlError) {
          return (
            <ErrorAlert>
              <div className='whitespace-pre-wrap font-mono text-xs'>{sqlError}</div>
            </ErrorAlert>
          );
        }
        if (generatedSql) {
          return (
            <SyntaxHighlighter
              language='sql'
              style={prismTheme}
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
