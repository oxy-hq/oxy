import SemanticQueryPanel from "@/components/SemanticQueryPanel";
import type { Block } from "@/services/types";
import type { SemanticQueryArtifact } from "@/types/artifact";
import Warning from "./Warning";

type SemanticQueryValue = SemanticQueryArtifact["content"]["value"];

function parseSemanticQuery(raw: string): Record<string, unknown> | null {
  try {
    return JSON.parse(raw);
  } catch {
    return null;
  }
}

function buildSemanticQueryArtifact(
  block: Block & { type: "semantic_query" },
  parsed: Record<string, unknown>
): SemanticQueryArtifact {
  const topic = (parsed.topic as string) || "";
  return {
    id: block.id,
    name: topic || "semantic_query",
    kind: "semantic_query",
    content: {
      type: "semantic_query",
      value: {
        database: "default",
        sql_query: block.sql_query || "",
        result: block.results,
        is_result_truncated: false,
        topic,
        dimensions: (parsed.dimensions as string[]) || [],
        measures: (parsed.measures as string[]) || [],
        filters: (parsed.filters as SemanticQueryValue["filters"]) || [],
        orders: (parsed.orders as SemanticQueryValue["orders"]) || [],
        limit: parsed.limit as number | undefined,
        offset: parsed.offset as number | undefined
      }
    }
  };
}

interface AgenticSemanticQueryPanelProps {
  block: Block & { type: "semantic_query" };
  onRerun?: (prompt: string) => void;
}

const AgenticSemanticQueryPanel = ({ block, onRerun }: AgenticSemanticQueryPanelProps) => {
  const parsed = parseSemanticQuery(block.semantic_query);
  if (!parsed) return <Warning message='Invalid semantic query content' />;
  const artifact = buildSemanticQueryArtifact(block, parsed);

  return <SemanticQueryPanel artifact={artifact} editable={true} onRerun={onRerun} />;
};

export default AgenticSemanticQueryPanel;
