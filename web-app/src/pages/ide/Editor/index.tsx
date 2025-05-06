import { useParams } from "react-router-dom";
import AgentEditor from "./Agent";
import AppEditor from "./App";
import DefaultEditor from "./Default";
import WorkflowEditor from "./Workflow";
import SqlEditor from "./Sql";

export const Editor = ({ pathb64 }: { pathb64: string }) => {
  const filePath = atob(pathb64 ?? "");
  const isWorkflow = filePath.endsWith(".workflow.yml");
  const isAgent = filePath.endsWith(".agent.yml");
  const isApp = filePath.endsWith(".app.yml");
  const isSql = filePath.endsWith(".sql");

  switch (true) {
    case isWorkflow:
      return <WorkflowEditor pathb64={pathb64} />;
    case isAgent:
      return <AgentEditor pathb64={pathb64} />;
    case isApp:
      return <AppEditor pathb64={pathb64} />;
    case isSql:
      return <SqlEditor pathb64={pathb64} />;
    default:
      return <DefaultEditor pathb64={pathb64} />;
  }
};

const EditorPage = () => {
  const { pathb64 } = useParams();
  return <Editor key={pathb64 ?? ""} pathb64={pathb64 ?? ""} />;
};

export default EditorPage;
