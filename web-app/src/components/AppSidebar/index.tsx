import { useCallback, useRef, useState } from "react";
import { useLocation } from "react-router-dom";
import {
  Sidebar as ShadcnSidebar,
  SidebarGroup,
  SidebarMenu
} from "@/components/ui/shadcn/sidebar";
import { Apps } from "./Apps";
import { Footer } from "./Footer";
import { Header } from "./Header";
import Threads from "./Threads";
import { Workflows } from "./Workflows";

export function AppSidebar() {
  const location = useLocation();
  const isOnboarding = location.pathname.includes("/onboarding");
  const [isScrolled, setIsScrolled] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);

  const handleScroll = useCallback(() => {
    if (scrollRef.current) {
      setIsScrolled(scrollRef.current.scrollTop > 0);
    }
  }, []);

  return (
    <ShadcnSidebar className='border-sidebar-border border-r bg-sidebar-background'>
      <Header isOnboarding={isOnboarding} />

      <div className='relative min-h-0 flex-1'>
        {isOnboarding ? (
          <div className='flex h-full items-center justify-center px-4'>
            <p className='text-center text-muted-foreground/50 text-xs'>
              Setting up your workspace...
            </p>
          </div>
        ) : (
          <div
            ref={scrollRef}
            onScroll={handleScroll}
            className='scrollbar-gutter-auto flex h-full flex-col overflow-auto'
          >
            <SidebarGroup className='mb-10 px-2 pt-2'>
              <SidebarMenu>
                <Threads />
                <Workflows />
                <Apps />
              </SidebarMenu>
            </SidebarGroup>
          </div>
        )}
        <div
          className={`pointer-events-none absolute top-0 right-0 left-0 z-10 h-10 bg-gradient-to-b from-sidebar-background to-transparent transition-opacity ${isScrolled ? "opacity-100" : "opacity-0"}`}
        />
        <div className='pointer-events-none absolute right-0 bottom-0 left-0 z-10 h-16 bg-gradient-to-t from-sidebar-background to-transparent' />
      </div>

      <Footer />
    </ShadcnSidebar>
  );
}
