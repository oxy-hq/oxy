import React from "react";

import { open } from "@tauri-apps/plugin-dialog";
import { useNavigate } from "react-router-dom";
import { css } from "styled-system/css";

import Button from "@/components/ui/Button";
import Icon from "@/components/ui/Icon";
import { toast } from "@/components/ui/Toast";
import useProjectPath from "@/stores/useProjectPath";

const wrapperStyles = css({
  display: "flex",
  flexDirection: "column",
  alignItems: "center",
  justifyContent: "center",
  h: "100%",
  w: "100%",
  backgroundColor: "neutral.bg.colorBg",
});

const Init: React.FC = () => {
  const setProjectPath = useProjectPath((state) => state.setProjectPath);
  const navigate = useNavigate();

  const handleFolderSelect = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Select Project Folder",
      });

      if (selected) {
        const folderPath = Array.isArray(selected) ? selected[0] : selected;
        setProjectPath(folderPath);
        navigate("/");
      }
    } catch (error) {
      console.error("Error selecting folder:", error);
      toast({
        title: "Error",
        description: "Error selecting folder, please try again",
      });
    }
  };

  return (
    <div className={wrapperStyles}>
      <Button
        size="large"
        variant="primary"
        content="iconText"
        onClick={handleFolderSelect}
      >
        <Icon asset="folder" />
        Open a folder
      </Button>
    </div>
  );
};

export default Init;
