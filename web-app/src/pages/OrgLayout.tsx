import { Outlet } from "react-router-dom";
import OrgSidebar from "@/components/OrgSidebar";

export default function OrgLayout() {
  return (
    <>
      <OrgSidebar />
      <main className='flex h-full min-w-0 flex-1 flex-col bg-background'>
        <div className='min-w-0 flex-1 overflow-auto'>
          <Outlet />
        </div>
      </main>
    </>
  );
}
