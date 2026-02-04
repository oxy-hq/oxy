import { memo } from "react";
import { FileType } from "@/utils/fileTypes";
import AgentEditor from "./Agent";
import AppEditor from "./App";
import { useEditorContext } from "./contexts/useEditorContext";
import DefaultEditor from "./Default";
import SqlEditor from "./Sql";
import TopicEditor from "./Topic";
import ViewEditor from "./View";
import WorkflowEditor from "./Workflow";

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
    default:
      return <DefaultEditor />;
  }
};

export const EditorRouter = memo(EditorRouterComponent);
