import React from "react";

const TableWrapper: React.FC<React.PropsWithChildren> = ({ children }) => {
  return <div className="border rounded-lg">{children}</div>;
};

export default TableWrapper;
