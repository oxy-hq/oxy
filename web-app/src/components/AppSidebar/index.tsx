import { useCallback, useRef, useState } from "react";
import {
  Sidebar as ShadcnSidebar,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarMenu
} from "@/components/ui/shadcn/sidebar";
import { Apps } from "./Apps";
import { Footer } from "./Footer";
import { Header } from "./Header";
import Threads from "./Threads";
import { Workflows } from "./Workflows";

export function AppSidebar() {
  const [isScrolled, setIsScrolled] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);

  const handleScroll = useCallback(() => {
    if (scrollRef.current) {
      setIsScrolled(scrollRef.current.scrollTop > 0);
    }
  }, []);

  return (
    <ShadcnSidebar className='border-border border-r bg-sidebar-background'>
      <Header />

      <div className='relative min-h-0 flex-1'>
        <div
          ref={scrollRef}
          onScroll={handleScroll}
          className='customScrollbar scrollbar-gutter-auto flex h-full flex-col overflow-auto'
        >
          <SidebarGroup className='mb-10 px-2 pt-0'>
            <SidebarGroupLabel>Workspace</SidebarGroupLabel>
            <SidebarMenu>
              <Threads />
              <Workflows />
              <Apps />
            </SidebarMenu>
          </SidebarGroup>
        </div>
        <div
          className={`pointer-events-none absolute top-0 right-0 left-0 z-10 h-16 bg-gradient-to-b from-sidebar-background to-transparent transition-opacity ${isScrolled ? "opacity-100" : "opacity-0"}`}
        />
        <div className='pointer-events-none absolute right-0 bottom-0 left-0 z-10 h-16 bg-gradient-to-t from-sidebar-background to-transparent' />
      </div>

      <Footer />
    </ShadcnSidebar>
  );
}
