import { useEffect, useRef, useState } from "react";

import { useAutoAnimate } from "@formkit/auto-animate/react";
import { NavLink } from "react-router-dom";
import { css, cx } from "styled-system/css";

import SidebarItem from "@/components/ui/Sidebar/SidebarItem";
import { Conversation } from "@/types/chat";

export interface SidebarItemGroupProps {
  items: Conversation[];
  isMobile?: boolean;
}

const scrollableChatListStyles = css({
  paddingRight: "md",
  overflowY: "auto",
  customScrollbar: true,
  display: "flex",
  flexDirection: "column",
  gap: "xxs",
  "&::-webkit-scrollbar": {
    display: "none",
  },
  "&:hover::-webkit-scrollbar": {
    display: "block",
  },
});

const extraScrollableChatListStyles = css({
  "&:hover": {
    paddingRight: "6px",
  },
});

export default function ChatList({ items }: SidebarItemGroupProps) {
  const [parent] = useAutoAnimate({ duration: 400 });
  const [shouldShowScrollbar, setShouldShowScrollbar] =
    useState<boolean>(false);
  const chatListRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const currentRef = chatListRef.current;
    if (currentRef) {
      currentRef.addEventListener("mouseover", handleHoverElement);
    }
    return () => {
      if (currentRef) {
        currentRef.removeEventListener("mouseover", handleHoverElement);
      }
    };
  }, [chatListRef]);

  const handleHoverElement = () => {
    if (
      Number(chatListRef.current?.scrollHeight) >
      Number(chatListRef.current?.clientHeight)
    ) {
      setShouldShowScrollbar(true);
    }
  };

  const extraWrapperStyles = shouldShowScrollbar
    ? extraScrollableChatListStyles
    : null;

  return (
    <div
      className={cx(scrollableChatListStyles, extraWrapperStyles)}
      ref={chatListRef}
    >
      <div ref={parent}>
        {items.map(({ id, title, agent }) => {
          return (
            <NavLink key={id} to={"/chat/" + btoa(agent)}>
              {({ isActive }) => (
                <SidebarItem
                  key={id}
                  title={title}
                  id={id}
                  isActive={isActive}
                />
              )}
            </NavLink>
          );
        })}
      </div>
    </div>
  );
}
