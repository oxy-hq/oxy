"use client";

import { useEffect, useRef } from "react";

import { css } from "styled-system/css";

import useSidebarState from "@/stores/useSidebarState";

import Button from "../ui/Button";
import Icon from "../ui/Icon";

const wrapperStyles = css({
  display: {
    base: "block",
    sm: "none",
  },
  width: "100%",
  px: "xl",
  py: "sm",
  position: "fixed",
  top: 0,
  left: 0,
  right: 0,
  zIndex: 98,
  bg: "background.primary",
});

const navbarWrapperStyles = css({
  display: "flex",
  flexDir: "row",
  justifyContent: "space-between",
  alignItems: "center",
});

export default function MobileTopBar() {
  const topBarRef = useRef<HTMLDivElement | null>(null);
  const toggleSidebar = useSidebarState((state) => state.toggle);

  useEffect(() => {
    const handleResize = () => {
      if (window.visualViewport && topBarRef.current) {
        // Adjust the transform to compensate for the viewport height change
        // This is necessary in ios due to the keyboard resizing the viewport
        const offsetTop = window.visualViewport.offsetTop;
        topBarRef.current.style.top = `${offsetTop}px`;
      }
    };

    if (window.visualViewport) {
      window.visualViewport.addEventListener("resize", handleResize);
      handleResize(); // Set initial position
    }

    // Cleanup the event listener when the component unmounts
    return () => {
      if (window.visualViewport) {
        window.visualViewport.removeEventListener("resize", handleResize);
      }
    };
  }, []);

  return (
    <nav id="mobile-top-bar" ref={topBarRef} className={wrapperStyles}>
      <div className={navbarWrapperStyles}>
        <Button
          onClick={toggleSidebar}
          content="icon"
          variant="outline"
          size="large"
        >
          <Icon asset="menu" />
        </Button>
      </div>
    </nav>
  );
}
