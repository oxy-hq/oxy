"use client";

import { AgentWithBg } from "../ui/Icon/CustomIcons/AgentWithBg";
import FileTree from "./FileTree";
import { FileTreeProvider } from "./FileTree/FileTreeContext";
import {
  mainSidebarContentStyles,
  sidebarInnerStyles,
  sidebarNavigationItems,
} from "./Leftsidebar.styles";
import { useState } from "react";
import OpenAIAPIKeyModal from "./OpenAIAPIKeyModal";

export default function LeftSidebarContent() {
  const [openOpenAIAPIKeyModal, setOpenOpenAIAPIKeyModal] = useState(false);

  return (
    <aside className={sidebarInnerStyles}>
      <OpenAIAPIKeyModal
        open={openOpenAIAPIKeyModal}
        setOpen={setOpenOpenAIAPIKeyModal}
      />
      <div className={mainSidebarContentStyles}>
        <AgentWithBg width={24} />
      </div>
      <div className={sidebarNavigationItems}>
        <FileTreeProvider>
          <FileTree
            openOpenAIAPIKeyModal={() => setOpenOpenAIAPIKeyModal(true)}
          />
        </FileTreeProvider>
      </div>
    </aside>
  );
}
