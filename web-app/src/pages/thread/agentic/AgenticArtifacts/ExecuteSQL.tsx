import { useMemo } from "react";
import SqlArtifactPanel from "@/components/ArtifactPanel/ArtifactsContent/sql";
import type { BlockBase, SqlContent } from "@/services/types";
import type { SqlArtifact } from "@/types/artifact";

const ExecuteSQL = ({ block }: { block: BlockBase & SqlContent }) => {
  const artifact: SqlArtifact = useMemo(() => {
    return {
      id: block.id,
      name: block.database,
      kind: "execute_sql",
      content: {
        type: "execute_sql",
        value: {
          database: block.database,
          sql_query: block.sql_query,
          result: block.result,
          is_result_truncated: block.is_result_truncated
        }
      }
    };
  }, [block]);

  return <SqlArtifactPanel artifact={artifact} />;
};

export default ExecuteSQL;
