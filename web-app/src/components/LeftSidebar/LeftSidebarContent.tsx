"use client";

import { css } from "styled-system/css";

import useTheme from "@/stores/useTheme";

import { ToggleButton } from "../ui/ActionButton";
import { AgentWithBg } from "../ui/Icon/CustomIcons/AgentWithBg";
import FileTree from "./FileTree";
import { FileTreeProvider } from "./FileTree/FileTreeContext";
import {
  mainSidebarContentStyles,
  sidebarInnerStyles,
  sidebarNavigationItems,
} from "./Leftsidebar.styles";

const bottomActionsStyles = css({
  display: "flex",

  gap: "xs",
  flexDirection: "column",
  pr: "md",
});

export default function LeftSidebarContent() {
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
        <AgentWithBg width={24} />
      </div>
      <div className={sidebarNavigationItems}>
        <FileTreeProvider>
          <FileTree />
        </FileTreeProvider>
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
