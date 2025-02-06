import {
  desktopSidebarWrapperStyles,
  sidebarStyles,
} from "./Leftsidebar.styles";
import LeftSidebarContent from "./LeftSidebarContent";

export default function DesktopLeftSidebar({
  initialWidth,
}: {
  initialWidth: number;
}) {
  return (
    <div
      style={{ width: initialWidth }}
      className={desktopSidebarWrapperStyles}
    >
      <div style={{ width: initialWidth }} className={sidebarStyles}>
        <LeftSidebarContent />
      </div>
    </div>
  );
}
