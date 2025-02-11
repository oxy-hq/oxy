import { Outlet } from "react-router-dom";

import LeftSidebar from "@/components/LeftSidebar";
import MobileTopBar from "@/components/MobileTopBar";

export default function WithSidebarLayout() {
  return (
    <>
      <MobileTopBar />
      <LeftSidebar />
      <Outlet />
    </>
  );
}
