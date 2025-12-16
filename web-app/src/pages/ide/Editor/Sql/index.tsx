import { useState } from "react";
import useExecuteSql from "@/hooks/api/useExecuteSql";
import HeaderActions from "./HeaderActions";
import Results from "./Results";
import EditorPageWrapper from "../components/EditorPageWrapper";
import { useEditorContext } from "../contexts/useEditorContext";
import { Button } from "@/components/ui/shadcn/button";
import { Download } from "lucide-react";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/shadcn/tooltip";
import Papa from "papaparse";
import { handleDownloadFile } from "@/libs/utils/string";

const SqlEditor = () => {
  const { pathb64, isReadOnly, gitEnabled } = useEditorContext();
  const [result, setResult] = useState<string[][]>([]);
  const [sql, setSql] = useState("");
  const { mutate: executeSql, isPending: loading } = useExecuteSql();

  const handleExecuteSql = (database: string) => {
    executeSql(
      {
        pathb64,
        sql,
        database,
      },
      {
        onSuccess: (data) => setResult(data),
      },
    );
  };

  return (
    <EditorPageWrapper
      pathb64={pathb64}
      onChanged={setSql}
      readOnly={isReadOnly}
      git={gitEnabled}
      defaultDirection="vertical"
      headerActions={
        <HeaderActions onExecuteSql={handleExecuteSql} loading={loading} />
      }
      preview={
        <div className="flex-1 flex flex-col overflow-hidden">
          {result.length > 0 && (
            <div className="flex items-center justify-end px-4 py-2 border-b">
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => {
                      const csvContent = Papa.unparse(result, {
                        delimiter: ",",
                        header: true,
                        skipEmptyLines: true,
                      });
                      const blob = new Blob([csvContent], {
                        type: "text/csv;charset=utf-8;",
                      });
                      handleDownloadFile(blob, "query_results.csv");
                    }}
                    className="h-7 w-7 p-0"
                  >
                    <Download className="w-4 h-4" />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>Download results as CSV</TooltipContent>
              </Tooltip>
            </div>
          )}
          <div className="flex-1 overflow-hidden">
            <Results result={result} />
          </div>
        </div>
      }
    />
  );
};

export default SqlEditor;
