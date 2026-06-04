import type React from "react";
import AuditTable from "./components/AuditTable";

const AuditPage: React.FC = () => (
  <div className='flex h-full min-h-0 flex-col gap-3 p-4'>
    <AuditTable />
  </div>
);

export default AuditPage;
