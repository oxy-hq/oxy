import type { Block } from "@/services/types";

export const ARTIFACT_TYPES = new Set(["sql", "semantic_query", "viz", "data_app"]);

export const PILL_CLASS =
  "flex shrink-0 items-center gap-1 rounded bg-muted/60 px-1.5 py-0.5 font-mono text-[11px] text-muted-foreground transition-colors hover:bg-muted hover:text-foreground";

const UUID_RE = /\s*[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\s*(\([^)]*\))?/gi;

export function stripUuids(text: string): string {
  return text
    .replace(UUID_RE, "")
    .replace(/,\s*,/g, ",")
    .replace(/\s{2,}/g, " ")
    .trim();
}

export function findArtifactBlock(blocks: Block[]): Block | null {
  return blocks.find((b) => ARTIFACT_TYPES.has(b.type)) ?? null;
}

export function getArtifactLabel(block: Block): string {
  if (block.type === "semantic_query") return "Semantic query";
  if (block.type === "sql") return "SQL query";
  return block.type;
}
