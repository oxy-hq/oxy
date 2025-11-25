import { useState } from "react";
import useExecuteSql from "@/hooks/api/useExecuteSql";
import HeaderActions from "./HeaderActions";
import Results from "./Results";
import EditorPageWrapper from "../components/EditorPageWrapper";
import { useEditorContext } from "../contexts/useEditorContext";

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
        <HeaderActions
          onExecuteSql={handleExecuteSql}
          loading={loading}
          sql={sql}
        />
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
