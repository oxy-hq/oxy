import React from "react";

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
      const input = document.createElement("input");
      input.type = "file";
      input.webkitdirectory = true;
      input.onchange = (e) => {
        const folderPath = (
          e.target as HTMLInputElement
        ).files?.[0].webkitRelativePath.split("/")[0];
        if (folderPath) {
          setProjectPath(folderPath);
          navigate("/");
        }
      };
      input.click();
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
