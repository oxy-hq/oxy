import { useParams } from "react-router-dom";
import { useState } from "react";
import FileEditor, { FileState } from "@/components/FileEditor";
import useExecuteSql from "@/hooks/api/useExecuteSql";
import Header from "../Header";
import HeaderActions from "./HeaderActions";
import Results from "./Results";

const SqlEditorPage = () => {
  const { pathb64 } = useParams();
  const filePath = atob(pathb64 ?? "");
  const [fileState, setFileState] = useState<FileState>("saved");
  const [result, setResult] = useState<string[][]>([]);
  const [sql, setSql] = useState("");
  const { mutate: executeSql, isPending: loading } = useExecuteSql();
  const handleExecuteSql = (database: string) => {
    executeSql(
      {
        pathb64: pathb64 ?? "",
        sql,
        database,
      },
      {
        onSuccess: (data) => setResult(data),
      },
    );
  };

  return (
    <div className="flex h-full w-full flex-col">
      <div className="flex-1 flex flex-col w-full bg-[#1e1e1e]">
        <Header
          filePath={filePath}
          fileState={fileState}
          actions={
            <HeaderActions onExecuteSql={handleExecuteSql} loading={loading} />
          }
        />

        <FileEditor
          fileState={fileState}
          pathb64={pathb64 ?? ""}
          onFileStateChange={setFileState}
          onValueChange={setSql}
        />
      </div>

      <div className="flex-1 overflow-hidden">
        <Results result={result} />
      </div>
    </div>
  );
};

export default SqlEditorPage;
