import AppPreview from "@/components/AppPreview";
import { DisplayBlock } from "@/components/AppPreview/Displays";
import LookerQueryArtifactPanel from "@/components/ArtifactPanel/ArtifactsContent/looker-query";
import SqlArtifactPanel from "@/components/ArtifactPanel/ArtifactsContent/sql";
import Markdown from "@/components/Markdown";
import { encodeBase64 } from "@/libs/encoding";
import type { Block } from "@/services/types";
import type { Display, TableDisplay } from "@/types/app";
import type { LookerQueryArtifact, SqlArtifact } from "@/types/artifact";
import RoutePanel from "../../RoutePanel";
import AgenticSemanticQueryPanel from "./AgenticSemanticQueryPanel";
import SubGroupReasoningPanel from "./SubGroupReasoningPanel";
import Warning from "./Warning";

const ROUTE_NAME_RE = /Selected route: \*\*(.+?)\*\*/;

function buildSqlArtifact(block: Block & { type: "sql" }): SqlArtifact {
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
}

function buildLookerQueryArtifact(block: Block & { type: "looker_query" }): LookerQueryArtifact {
  return {
    id: block.id,
    name: `${block.model}.${block.explore}`,
    kind: "looker_query",
    content: {
      type: "looker_query",
      value: {
        model: block.model,
        explore: block.explore,
        sql: block.sql_query || block.sql || "",
        result: block.result,
        result_file: block.result_file,
        is_result_truncated: block.is_result_truncated,
        fields: block.fields,
        filters: block.filters,
        sorts: block.sorts,
        limit: block.limit
      }
    }
  };
}

interface ArtifactBlockRendererProps {
  block: Block;
  onRerun?: (prompt: string) => void;
}

const ArtifactBlockRenderer = ({ block, onRerun }: ArtifactBlockRendererProps) => {
  switch (block.type) {
    case "group":
      return <SubGroupReasoningPanel groupId={block.group_id} />;

    case "text": {
      const match = block.content.match(ROUTE_NAME_RE);
      if (match) return <RoutePanel routeName={match[1]} />;
      return <Markdown>{block.content}</Markdown>;
    }

    case "semantic_query":
      return <AgenticSemanticQueryPanel block={block} onRerun={onRerun} />;

    case "sql":
      return <SqlArtifactPanel artifact={buildSqlArtifact(block)} />;

    case "looker_query":
      return <LookerQueryArtifactPanel artifact={buildLookerQueryArtifact(block)} />;

    case "viz":
      return (
        <div className='p-4'>
          <DisplayBlock
            display={block.config as Display}
            data={{
              [(block.config as TableDisplay).data]: {
                file_path: (block.config as TableDisplay).data
              }
            }}
          />
        </div>
      );

    case "data_app":
      return (
        <div className='relative h-full'>
          <AppPreview appPath64={encodeBase64(block.file_path)} />
        </div>
      );

    default:
      return <Warning message={`Unsupported block type: ${block.type}`} />;
  }
};

export default ArtifactBlockRenderer;
