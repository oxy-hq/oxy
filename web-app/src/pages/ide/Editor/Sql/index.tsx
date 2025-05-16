import { useState } from "react";
import useExecuteSql from "@/hooks/api/useExecuteSql";
import HeaderActions from "./HeaderActions";
import Results from "./Results";
import EditorPageWrapper from "../components/EditorPageWrapper";

const SqlEditor = ({ pathb64 }: { pathb64: string }) => {
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
    <EditorPageWrapper
      pathb64={pathb64}
      onFileValueChange={setSql}
      pageContentClassName="flex-col"
      editorClassName={"h-1/2 w-full"}
      headerActions={
        <HeaderActions onExecuteSql={handleExecuteSql} loading={loading} />
      }
      preview={
        <div className="flex-1 overflow-hidden">
          <Results result={result} />
        </div>
      }
    />
  );
};

export default SqlEditor;
