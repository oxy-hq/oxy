import { useCallback, useMemo, useRef, useState } from "react";

/**
 * Ordered-list selection with shift-click range support — the spreadsheet
 * gesture operators expect from a dense data-grid. Shared by every admin
 * table with row selection + a bulk-action bar (compiles, customer apps).
 *
 * `ids` is the current row order; toggling with `shiftKey` selects every
 * row between the last anchor and the clicked row. The anchor resets to
 * the most recently (non-shift) toggled row. Selection state is kept as a
 * `Set` keyed by row id, and stale ids (rows that dropped out of `ids`
 * after a refetch) are transparently filtered out of `selected`.
 *
 * `filterStale: false` opts out of that filtering — required for the rollup
 * view's expanded-revision selection, whose ids are lazily loaded inside child
 * rows and never bubble back up into `ids` (so filtering against an empty `ids`
 * would silently drop every selection). Without it, batch "Promote selected"
 * is unreachable from the By-workspace view.
 */
export function useRowSelection(ids: string[], opts?: { filterStale?: boolean }) {
  const filterStale = opts?.filterStale ?? true;
  const [raw, setRaw] = useState<Set<string>>(new Set());
  const anchorRef = useRef<string | null>(null);

  // Only ids still present in the current list count as selected — this
  // keeps the bulk bar honest across polling refetches that drop rows.
  const selected = useMemo(() => {
    if (!filterStale) return raw;
    const present = new Set(ids);
    return new Set([...raw].filter((id) => present.has(id)));
  }, [raw, ids, filterStale]);

  const toggle = useCallback(
    (id: string, shiftKey: boolean) => {
      setRaw((prev) => {
        const next = new Set(prev);
        const anchor = anchorRef.current;
        if (shiftKey && anchor && anchor !== id) {
          const start = ids.indexOf(anchor);
          const end = ids.indexOf(id);
          if (start !== -1 && end !== -1) {
            const [lo, hi] = start < end ? [start, end] : [end, start];
            const shouldSelect = !next.has(id);
            for (let i = lo; i <= hi; i++) {
              if (shouldSelect) next.add(ids[i]);
              else next.delete(ids[i]);
            }
            return next;
          }
        }
        if (next.has(id)) next.delete(id);
        else next.add(id);
        anchorRef.current = id;
        return next;
      });
    },
    [ids]
  );

  const allSelected = ids.length > 0 && ids.every((id) => selected.has(id));
  const someSelected = selected.size > 0 && !allSelected;

  const toggleAll = useCallback(() => {
    setRaw((prev) => {
      const everySelected = ids.length > 0 && ids.every((id) => prev.has(id));
      if (everySelected) return new Set();
      return new Set(ids);
    });
    anchorRef.current = null;
  }, [ids]);

  const clear = useCallback(() => {
    setRaw(new Set());
    anchorRef.current = null;
  }, []);

  // Bulk add/remove a known subset in one update — used for group-level
  // "select all in this group" toggles, where flipping each id individually
  // would thrash state and fight the anchor.
  const setMany = useCallback((subset: string[], selected: boolean) => {
    setRaw((prev) => {
      const next = new Set(prev);
      for (const id of subset) {
        if (selected) next.add(id);
        else next.delete(id);
      }
      return next;
    });
  }, []);

  return {
    selected,
    selectedIds: useMemo(() => [...selected], [selected]),
    isSelected: useCallback((id: string) => selected.has(id), [selected]),
    toggle,
    toggleAll,
    setMany,
    clear,
    allSelected,
    someSelected
  };
}
