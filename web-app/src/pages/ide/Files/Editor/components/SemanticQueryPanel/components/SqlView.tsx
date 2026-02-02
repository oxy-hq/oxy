import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneDark } from "react-syntax-highlighter/dist/esm/styles/prism";

interface SqlViewProps {
  generatedSql: string;
  sqlError: string | null;
}

const SqlView = ({ generatedSql, sqlError }: SqlViewProps) => {
  return (
    <div className="h-full overflow-auto customScrollbar p-4">
      {(() => {
        if (sqlError) {
          return (
            <div className="text-xs font-mono bg-destructive/10 text-destructive p-4 rounded whitespace-pre-wrap">
              {sqlError}
            </div>
          );
        }
        if (generatedSql) {
          return (
            <SyntaxHighlighter
              language="sql"
              style={oneDark}
              customStyle={{ margin: 0, borderRadius: "0.5rem" }}
              className="text-xs font-mono"
            >
              {generatedSql}
            </SyntaxHighlighter>
          );
        }
        return (
          <div className="flex items-center justify-center h-full text-sm text-muted-foreground">
            Run a query to see the generated SQL
          </div>
        );
      })()}
    </div>
  );
};

export default SqlView;
