import type { editor } from "monaco-editor";
import { useEffect, useRef } from "react";

// Monaco decoration class names (styled in shadcn/index.css)
// TODO: The overviewRuler rgba colors below (green for added, orange for modified)
// could be migrated to CSS variables with color-mix(), e.g.
// `color-mix(in srgb, var(--success) 80%, transparent)`, but are left as-is since
// Monaco's overviewRuler.color expects a direct color string, not a CSS variable.
const ADDED_CLASS = "gutter-line-added";
const MODIFIED_CLASS = "gutter-line-modified";
const DELETED_CLASS = "gutter-line-deleted";

interface LineChanges {
  added: number[]; // 1-indexed
  modified: number[]; // 1-indexed
  deletedBefore: number[]; // 1-indexed — deletion occurred before this line
}

/**
 * Computes line-level diff between `original` and `modified` strings.
 *
 * Uses LCS (longest common subsequence) for files up to MAX_LINES combined
 * lines; falls back to a positional comparison for larger files to keep the
 * cost bounded.
 */
function computeLineChanges(original: string, modified: string): LineChanges {
  const origLines = original.split("\n");
  const modLines = modified.split("\n");
  const m = origLines.length;
  const n = modLines.length;

  // Positional fallback for very large files
  if (m + n > 3000) {
    const added: number[] = [];
    const modifiedLines: number[] = [];
    for (let i = 0; i < n; i++) {
      if (i >= m) {
        added.push(i + 1);
      } else if (modLines[i] !== origLines[i]) {
        modifiedLines.push(i + 1);
      }
    }
    return { added, modified: modifiedLines, deletedBefore: [] };
  }

  // LCS DP table (bottom-up)
  const dp: number[][] = Array.from({ length: m + 1 }, () => new Array(n + 1).fill(0));
  for (let i = m - 1; i >= 0; i--) {
    for (let j = n - 1; j >= 0; j--) {
      dp[i][j] =
        origLines[i] === modLines[j] ? 1 + dp[i + 1][j + 1] : Math.max(dp[i + 1][j], dp[i][j + 1]);
    }
  }

  // Backtrack to produce edit operations
  type Op =
    | { type: "equal"; orig: number; mod: number }
    | { type: "delete"; orig: number }
    | { type: "insert"; mod: number };

  const ops: Op[] = [];
  let i = 0,
    j = 0;
  while (i < m && j < n) {
    if (origLines[i] === modLines[j]) {
      ops.push({ type: "equal", orig: i, mod: j });
      i++;
      j++;
    } else if (dp[i + 1][j] >= dp[i][j + 1]) {
      ops.push({ type: "delete", orig: i });
      i++;
    } else {
      ops.push({ type: "insert", mod: j });
      j++;
    }
  }
  while (i < m) {
    ops.push({ type: "delete", orig: i });
    i++;
  }
  while (j < n) {
    ops.push({ type: "insert", mod: j });
    j++;
  }

  // Classify ops into decoration buckets
  const added: number[] = [];
  const modifiedLines: number[] = [];
  const deletedBefore: number[] = [];

  let k = 0;
  while (k < ops.length) {
    const op = ops[k];
    if (op.type === "equal") {
      k++;
      continue;
    }
    if (op.type === "delete") {
      // delete immediately followed by insert → replace (modified)
      if (k + 1 < ops.length && ops[k + 1].type === "insert") {
        modifiedLines.push((ops[k + 1] as { mod: number }).mod + 1);
        k += 2;
        continue;
      }
      // Pure delete — show a marker above the next modified-file line
      let nextModLine = n + 1;
      for (let l = k + 1; l < ops.length; l++) {
        const next = ops[l];
        if (next.type !== "delete") {
          nextModLine = (next as { mod: number }).mod + 1;
          break;
        }
      }
      deletedBefore.push(nextModLine);
      k++;
      continue;
    }
    // pure insert
    added.push((op as { mod: number }).mod + 1);
    k++;
  }

  return { added, modified: modifiedLines, deletedBefore };
}

/**
 * Applies VS Code-style gutter decorations (added / modified / deleted) to a
 * Monaco editor instance.
 *
 * @param editorInstance  The Monaco `IStandaloneCodeEditor` (or null when not yet mounted).
 * @param content         The current editor content.
 * @param originalContent The git-committed content to diff against.
 * @param enabled         When false the decorations are removed immediately.
 */
export function useGutterDecorations(
  editorInstance: editor.IStandaloneCodeEditor | null,
  content: string,
  originalContent: string | undefined,
  enabled: boolean
) {
  const decorationsRef = useRef<editor.IEditorDecorationsCollection | null>(null);

  useEffect(() => {
    if (!editorInstance || !enabled || !originalContent) {
      decorationsRef.current?.clear();
      decorationsRef.current = null;
      return;
    }

    // Debounce the LCS computation (O(m×n)) to avoid blocking the input loop
    // on every keystroke. 200 ms is imperceptible for gutter updates.
    const timer = setTimeout(() => {
      const { added, modified, deletedBefore } = computeLineChanges(originalContent, content);

      const decorations: editor.IModelDeltaDecoration[] = [
        ...added.map((line) => ({
          range: {
            startLineNumber: line,
            endLineNumber: line,
            startColumn: 1,
            endColumn: 1
          },
          options: {
            isWholeLine: true,
            className: `${ADDED_CLASS}-bg`,
            linesDecorationsClassName: ADDED_CLASS,
            overviewRuler: {
              color: "color-mix(in srgb, var(--success) 80%, transparent)",
              position: 7 // OverviewRulerLane.Right
            }
          }
        })),
        ...modified.map((line) => ({
          range: {
            startLineNumber: line,
            endLineNumber: line,
            startColumn: 1,
            endColumn: 1
          },
          options: {
            isWholeLine: true,
            className: `${MODIFIED_CLASS}-bg`,
            linesDecorationsClassName: MODIFIED_CLASS,
            overviewRuler: {
              color: "color-mix(in srgb, var(--warning) 80%, transparent)",
              position: 7
            }
          }
        })),
        ...deletedBefore.map((line) => ({
          range: {
            startLineNumber: line,
            endLineNumber: line,
            startColumn: 1,
            endColumn: 1
          },
          options: {
            linesDecorationsClassName: DELETED_CLASS
          }
        }))
      ];

      if (!decorationsRef.current) {
        decorationsRef.current = editorInstance.createDecorationsCollection(decorations);
      } else {
        decorationsRef.current.set(decorations);
      }
    }, 200);

    return () => {
      clearTimeout(timer);
      decorationsRef.current?.clear();
      decorationsRef.current = null;
    };
  }, [editorInstance, content, originalContent, enabled]);
}
