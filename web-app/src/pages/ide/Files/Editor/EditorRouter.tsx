import { memo } from "react";
import { FileType } from "@/utils/fileTypes";
import AgentEditor from "./Agent";
import AgenticAnalyticsEditor from "./AgenticAnalytics";
import AppEditor from "./App";
import { useEditorContext } from "./contexts/useEditorContext";
import DefaultEditor from "./Default";
import MarkdownEditor from "./Markdown";
import SqlEditor from "./Sql";
import TestFileEditor from "./TestFile";
import TopicEditor from "./Topic";
import ViewEditor from "./View";
import WorkflowEditor from "./Workflow";

const EditorRouterComponent = () => {
  const { fileType } = useEditorContext();

  switch (fileType) {
    case FileType.PROCEDURE:
    case FileType.WORKFLOW:
    case FileType.AUTOMATION:
      return <WorkflowEditor />;
    case FileType.AGENT:
      return <AgentEditor />;
    case FileType.ANALYTICS_AGENT:
      return <AgenticAnalyticsEditor />;
    case FileType.APP:
      return <AppEditor />;
    case FileType.SQL:
      return <SqlEditor />;
    case FileType.VIEW:
      return <ViewEditor />;
    case FileType.TOPIC:
      return <TopicEditor />;
    case FileType.TEST:
      return <TestFileEditor />;
    case FileType.MARKDOWN:
      return <MarkdownEditor />;
    default:
      return <DefaultEditor />;
  }
};

export const EditorRouter = memo(EditorRouterComponent);
