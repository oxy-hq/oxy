import { decodeBase64 } from "@/libs/encoding";

export enum FileType {
  PROCEDURE = "procedure",
  WORKFLOW = "workflow",
  AUTOMATION = "automation",
  AGENT = "agent",
  AGENTIC_WORKFLOW = "agentic_workflow",
  ANALYTICS_AGENT = "analytics_agent",
  APP = "app",
  SQL = "sql",
  VIEW = "view",
  TOPIC = "topic",
  TEST = "test",
  DEFAULT = "default"
}

export interface FileTypeConfig {
  type: FileType;
  extensions: string[];
  editorComponent: string;
}

export const FILE_TYPE_CONFIGS: Record<FileType, FileTypeConfig> = {
  [FileType.TEST]: {
    type: FileType.TEST,
    extensions: [".test.yml", ".test.yaml"],
    editorComponent: "TestEditor"
  },
  [FileType.PROCEDURE]: {
    type: FileType.PROCEDURE,
    extensions: [".procedure.yml", ".procedure.yaml"],
    editorComponent: "WorkflowEditor"
  },
  [FileType.WORKFLOW]: {
    type: FileType.WORKFLOW,
    extensions: [".workflow.yml", ".workflow.yaml", ".automation.yml", ".automation.yaml"],
    editorComponent: "WorkflowEditor"
  },
  [FileType.AUTOMATION]: {
    type: FileType.AUTOMATION,
    extensions: [".automation.yml", ".automation.yaml"],
    editorComponent: "WorkflowEditor"
  },
  [FileType.AGENT]: {
    type: FileType.AGENT,
    extensions: [".agent.yml", ".agent.yaml"],
    editorComponent: "AgentEditor"
  },
  [FileType.AGENTIC_WORKFLOW]: {
    type: FileType.AGENTIC_WORKFLOW,
    extensions: [".aw.yml", ".aw.yaml"],
    editorComponent: "WorkflowEditor"
  },
  [FileType.ANALYTICS_AGENT]: {
    type: FileType.ANALYTICS_AGENT,
    extensions: [".agentic.yml", ".agentic.yaml"],
    editorComponent: "AgenticAnalyticsEditor"
  },
  [FileType.APP]: {
    type: FileType.APP,
    extensions: [".app.yml", ".app.yaml"],
    editorComponent: "AppEditor"
  },
  [FileType.SQL]: {
    type: FileType.SQL,
    extensions: [".sql"],
    editorComponent: "SqlEditor"
  },
  [FileType.VIEW]: {
    type: FileType.VIEW,
    extensions: [".view.yml", ".view.yaml"],
    editorComponent: "ViewEditor"
  },
  [FileType.TOPIC]: {
    type: FileType.TOPIC,
    extensions: [".topic.yml", ".topic.yaml"],
    editorComponent: "TopicEditor"
  },
  [FileType.DEFAULT]: {
    type: FileType.DEFAULT,
    extensions: [],
    editorComponent: "DefaultEditor"
  }
};

export const detectFileType = (filePath: string): FileType => {
  const lowerPath = filePath.toLowerCase();

  for (const config of Object.values(FILE_TYPE_CONFIGS)) {
    if (config.extensions.some((ext) => lowerPath.endsWith(ext))) {
      return config.type;
    }
  }

  return FileType.DEFAULT;
};

export const decodeFilePath = (pathb64: string): string => {
  try {
    return decodeBase64(pathb64);
  } catch {
    return "";
  }
};
