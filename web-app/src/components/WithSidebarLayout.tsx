import { Outlet } from "react-router-dom";

import LeftSidebar from "@/components/LeftSidebar";
import MobileTopBar from "@/components/MobileTopBar";
import { css } from "styled-system/css";

export default function WithSidebarLayout() {
  return (
    <>
      <MobileTopBar />
      <LeftSidebar />
      <div
        className={css({
          flex: 1,
        })}
      >
        <Outlet />
      </div>
    </>
  );
}
