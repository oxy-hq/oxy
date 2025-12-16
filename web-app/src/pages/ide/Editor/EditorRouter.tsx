import { memo } from "react";
import { FileType } from "@/utils/fileTypes";
import AgentEditor from "./Agent";
import AppEditor from "./App";
import DefaultEditor from "./Default";
import WorkflowEditor from "./Workflow";
import SqlEditor from "./Sql";
import ViewEditor from "./View";
import TopicEditor from "./Topic";
import { useEditorContext } from "./contexts/useEditorContext";

const EditorRouterComponent = () => {
  const { fileType } = useEditorContext();

  switch (fileType) {
    case FileType.WORKFLOW:
      return <WorkflowEditor />;
    case FileType.AGENT:
      return <AgentEditor />;
    case FileType.APP:
      return <AppEditor />;
    case FileType.SQL:
      return <SqlEditor />;
    case FileType.VIEW:
      return <ViewEditor />;
    case FileType.TOPIC:
      return <TopicEditor />;
    case FileType.DEFAULT:
    default:
      return <DefaultEditor />;
  }
};

export const EditorRouter = memo(EditorRouterComponent);
