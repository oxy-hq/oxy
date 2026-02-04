import type React from "react";

const TableWrapper: React.FC<React.PropsWithChildren> = ({ children }) => {
  return <div className='rounded-lg border'>{children}</div>;
};

export default TableWrapper;
