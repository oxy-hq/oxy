import useSidebarState from "@/stores/useSidebarState";

import Button from "../ui/Button";
import Icon from "../ui/Icon";
import {
  collapsedMobileSidebarStyles,
  mobileSidebarStyles,
  mobileSidebarWrapperStyles,
  sidebarMobileBackdropStyles,
} from "./Leftsidebar.styles";
import LeftSidebarContent from "./LeftSidebarContent";

export default function MobileLeftSidebar() {
  const { state: sidebarState, toggle: toggleSidebar } = useSidebarState(
    (state) => state,
  );

  return (
    <div className={mobileSidebarWrapperStyles}>
      <div className={collapsedMobileSidebarStyles}>
        <Button
          onClick={toggleSidebar}
          content="icon"
          variant="outline"
          size="large"
        >
          <Icon asset="menu" />
        </Button>
      </div>
      <div
        className={mobileSidebarStyles({ state: sidebarState })}
        onMouseDown={(e) => e.preventDefault()}
      >
        <LeftSidebarContent />
      </div>

      <div
        onClick={toggleSidebar}
        tabIndex={0}
        className={sidebarMobileBackdropStyles({
          state: sidebarState,
        })}
      />
    </div>
  );
}
