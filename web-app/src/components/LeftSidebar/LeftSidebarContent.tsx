"use client";

import { NavLink } from "react-router-dom";
import { css } from "styled-system/css";

import useTheme from "@/stores/useTheme";

import { ActionButton, ToggleButton } from "../ui/ActionButton";
import { AgentWithBg } from "../ui/Icon/CustomIcons/AgentWithBg";
import ChatGroup from "./ChatGroup";
import {
  mainSidebarContentStyles,
  sidebarHeadStyles,
  sidebarInnerStyles,
  sidebarNavigationItems,
} from "./Leftsidebar.styles";

interface SidebarProps {
  isMobile?: boolean;
}

const bottomActionsStyles = css({
  display: "flex",
  gap: "xs",
  flexDirection: "column",
  pr: "md",
});

export default function LeftSidebarContent({ isMobile }: SidebarProps) {
  const { theme, setTheme } = useTheme();

  const isDarkMode = theme === "dark";

  const onCheckedChange = (checked: boolean) => {
    if (checked) {
      setTheme("dark");
    } else {
      setTheme("light");
    }
  };

  return (
    <aside className={sidebarInnerStyles}>
      <div className={mainSidebarContentStyles}>
        <NavLink
          to="/"
          className={css({
            px: "sm",
            py: "xs",
          })}
        >
          <AgentWithBg width={81} />
        </NavLink>

        <div className={sidebarHeadStyles}>
          <NavLink to="/">
            {({ isActive }) => (
              <ActionButton
                iconAsset="home"
                text="Home"
                variant={isActive ? "secondary" : "dark"}
              />
            )}
          </NavLink>
        </div>
      </div>
      <div className={sidebarNavigationItems}>
        <ChatGroup isMobile={isMobile} />
      </div>

      <div className={bottomActionsStyles}>
        <ToggleButton
          checked={isDarkMode}
          iconAsset="dark_mode"
          text="Dark Mode"
          onCheckedChange={onCheckedChange}
        />
      </div>
    </aside>
  );
}
