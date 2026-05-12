import type React from "react";
import "./TableWrapper.css";

/**
 * Wraps a settings table in a bordered card. On md+ it stays a horizontally
 * scrollable table; below md the table collapses into stacked cards via the
 * sibling stylesheet — each `<td>` becomes a labeled block using its
 * `data-label` attribute as the field name.
 *
 * Action cells (or any cells that should render without a header) just omit
 * `data-label`.
 */
const TableWrapper: React.FC<React.PropsWithChildren> = ({ children }) => {
  return (
    <div className='settings-table-wrapper w-full rounded-lg border md:overflow-x-auto'>
      {children}
    </div>
  );
};

export default TableWrapper;
