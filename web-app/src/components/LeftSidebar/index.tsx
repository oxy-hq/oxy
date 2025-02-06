import DesktopLeftSidebar from "./DesktopLeftSidebar";
import { DEFAULT_SIDEBAR_WIDTH } from "./Leftsidebar.styles";
import MobileLeftSidebar from "./MobileLeftSidebar";

export default function LeftSidebar() {
  return (
    <>
      <DesktopLeftSidebar initialWidth={DEFAULT_SIDEBAR_WIDTH} />
      <MobileLeftSidebar />
    </>
  );
}
