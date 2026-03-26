import type { FileStatus } from "@/types/file";

const DEFAULT_MESSAGE = "Auto-commit: Oxy changes";

/** Generate a concise commit message from the changed file list. */
export function generateFromDiff(files: FileStatus[]): string {
  if (files.length === 0) return DEFAULT_MESSAGE;

  const base = (p: string) => p.split("/").pop() ?? p;
  const fmt = (paths: string[], verb: string) => {
    const names = paths.slice(0, 2).map(base);
    const extra = paths.length > 2 ? ` (+${paths.length - 2})` : "";
    return `${verb} ${names.join(", ")}${extra}`;
  };

  const added = files.filter((f) => f.status === "A").map((f) => f.path);
  const modified = files.filter((f) => f.status === "M").map((f) => f.path);
  const deleted = files.filter((f) => f.status === "D").map((f) => f.path);

  const parts: string[] = [];
  if (added.length) parts.push(fmt(added, "add"));
  if (modified.length) parts.push(fmt(modified, "update"));
  if (deleted.length) parts.push(fmt(deleted, "remove"));

  const msg = parts.join("; ");
  return msg.length > 72 ? `${msg.slice(0, 69)}…` : msg;
}
