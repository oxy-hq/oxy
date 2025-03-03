import React, { useMemo } from "react";
import useWorkflow from "@/stores/useWorkflow";
import { css } from "styled-system/css";
import RightSideBarPanel from "./RightSideBarPanel"; // Import the new component

const RightSidebar = () => {
  const selectedNodeId = useWorkflow((state) => state.selectedNodeId);
  const getNode = useWorkflow((state) => state.getNode);
  const selectedNode = useMemo(
    () => getNode(selectedNodeId),
    [selectedNodeId, getNode],
  );
  if (!selectedNode) {
    return null;
  }
  return (
    <div
      className={css({
        display: "flex",
      })}
    >
      <RightSideBarContent nodeId={selectedNode.id} />
    </div>
  );
};

const RightSideBarContent: React.FC = ({ nodeId }) => {
  const getNode = useWorkflow((state) => state.getNode);
  const selectedNode = getNode(nodeId);
  if (!selectedNode) {
    return null;
  }
  if (selectedNode.parentId) {
    return (
      <>
        <RightSideBarPanel nodeId={selectedNode.id} />
        <RightSideBarContent nodeId={selectedNode.parentId} />
      </>
    );
  }
  return <RightSideBarPanel nodeId={selectedNode.id} />;
};

export default RightSidebar;
