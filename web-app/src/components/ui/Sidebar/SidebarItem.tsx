import { css, cx } from "styled-system/css";

import Icon from "@/components/ui/Icon";
import Text from "@/components/ui/Typography/Text";
import useSidebarState from "@/stores/useSidebarState";

const itemStyles = css({
  outline: "none",
  position: "relative",
  display: "flex",
  justifyContent: "space-between",
  flexDir: "row",
  color: "text.light",
  alignItems: "center",
  height: "4xl",
  borderRadius: "rounded",
  width: "100%",
  cursor: "pointer",
  pr: "xs",
  "&[data-active=true]": {
    bg: "background.primary",
    // border and shadow
    shadow:
      "inset 0 0 0 1px token(colors.border.primary), token(shadows.primary)",
    _hover: {
      bgColor: "surface.primary",
    },
  },
  "&[data-focus=true]": {
    bgColor: "surface.tertiary",
  },
  "&[data-menu-open=true]": {
    bgColor: "surface.tertiary",
  },
  xl: {
    _hover: {
      bgColor: "surface.tertiary",
    },
  },
});

const contentStyles = css({
  display: "flex",
  gap: "sm",
  alignItems: "center",
  minW: "0",
  flex: 1,
  py: "sm",
  pl: "sm",
});

const iconStyles = css({
  flexShrink: 0,
});

const textStyles = css({
  truncate: true,
});

interface SidebarItemProps {
  isActive: boolean;
  id: string;
  title: string;
}

export default function SidebarItem({
  isActive = false,
  title = "Untitled",
}: SidebarItemProps) {
  const { close: closeSideBar } = useSidebarState();

  const onItemClick = () => {
    closeSideBar();
  };

  return (
    <div
      data-functional
      className={cx(itemStyles, "group")}
      data-active={isActive}
    >
      <div
        onClick={onItemClick}
        aria-selected={isActive}
        className={contentStyles}
      >
        <Icon className={iconStyles} asset="question" />

        <Text className={textStyles} variant="label14Regular" color="light">
          {title}
        </Text>
      </div>
    </div>
  );
}
