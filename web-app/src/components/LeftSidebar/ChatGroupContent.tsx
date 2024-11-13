"use client";

import { css } from "styled-system/css";

import { Conversation } from "@/types/chat";

import Skeleton from "../ui/Skeleton";
import ChatList from "./ChatList";
import Section from "./Section";

export interface SidebarItemGroupProps {
  isMobile?: boolean;
  chats: Conversation[] | undefined;
  isLoading: boolean;
}

const wrapperStyles = css({
  display: "flex",
  flexDirection: "column",
  height: "100%",
  gap: "xxs"
});

const skeletonStyles = css({
  paddingRight: "2xl"
});

export default function ChatGroupContent({ isMobile, chats, isLoading }: SidebarItemGroupProps) {
  if (isLoading) {
    return <Skeleton className={skeletonStyles} lineCount={5} />;
  }

  if (!chats || chats.length === 0) {
    return null;
  }

  return (
    <div className={wrapperStyles}>
      <Section section='Agents' isActive={!!chats.length} />
      <ChatList items={chats} isMobile={isMobile} />
    </div>
  );
}

