import { useState } from "react";
import useExecuteSql from "@/hooks/api/useExecuteSql";
import HeaderActions from "./HeaderActions";
import EditorPageWrapper from "../components/EditorPageWrapper";
import { useEditorContext } from "../contexts/useEditorContext";
import SqlResultsTable from "@/components/sql/SqlResultsTable";
import useDatabaseClient from "@/stores/useDatabaseClient";
import { decodeFilePath } from "@/utils/fileTypes";

const SqlEditor = () => {
  const { pathb64, isReadOnly, gitEnabled } = useEditorContext();
  const [result, setResult] = useState<string[][]>([]);
  const [resultFile, setResultFile] = useState<string | undefined>(undefined);
  const [sql, setSql] = useState("");
  const { mutate: executeSql, isPending: loading } = useExecuteSql();
  const { updateTabByPath } = useDatabaseClient();
  const filePath = decodeFilePath(pathb64);

  const handleExecuteSql = (database: string) => {
    executeSql(
      {
        pathb64,
        sql,
        database,
      },
      {
        onSuccess: (data) => {
          console.log("SQL execution result", data);
          // Response is either string[][] (JSON format) or { file_name: string } (Arrow format)
          if (Array.isArray(data)) {
            setResult(data);
            setResultFile(undefined);
          } else if (typeof data === "object" && "file_name" in data) {
            setResultFile((data as { file_name: string }).file_name);
            setResult([]);
          }
        },
      },
    );
  };

  const onSaved = (content?: string) => {
    if (content) {
      updateTabByPath(filePath, content);
    }
  };

  return (
    <EditorPageWrapper
      pathb64={pathb64}
      onSaved={onSaved}
      onChanged={setSql}
      readOnly={isReadOnly}
      git={gitEnabled}
      defaultDirection="vertical"
      headerActions={
        <HeaderActions onExecuteSql={handleExecuteSql} loading={loading} />
      }
      preview={
        <div className="flex-1 flex flex-col overflow-hidden">
          <div className="flex-1 overflow-hidden">
            <SqlResultsTable result={result} resultFile={resultFile} />
          </div>
        </div>
      }
    />
  );
};

export default SqlEditor;
