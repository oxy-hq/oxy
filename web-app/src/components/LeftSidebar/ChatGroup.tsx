import { useConversations } from "@/hooks/api/useConversations";

import ChatGroupContent from "./ChatGroupContent";

export interface SidebarItemGroupProps {
  isMobile?: boolean;
}

export default function ChatGroup({ isMobile }: SidebarItemGroupProps) {
  const { data: chats, isLoading } = useConversations();

  return (
    <ChatGroupContent isMobile={isMobile} chats={chats} isLoading={isLoading} />
  );
}
