export enum FileType {
  WORKFLOW = "workflow",
  AGENT = "agent",
  APP = "app",
  SQL = "sql",
  DEFAULT = "default",
}

export interface FileTypeConfig {
  type: FileType;
  extensions: string[];
  editorComponent: string;
}

export const FILE_TYPE_CONFIGS: Record<FileType, FileTypeConfig> = {
  [FileType.WORKFLOW]: {
    type: FileType.WORKFLOW,
    extensions: [".workflow.yml", ".workflow.yaml"],
    editorComponent: "WorkflowEditor",
  },
  [FileType.AGENT]: {
    type: FileType.AGENT,
    extensions: [".agent.yml", ".agent.yaml"],
    editorComponent: "AgentEditor",
  },
  [FileType.APP]: {
    type: FileType.APP,
    extensions: [".app.yml", ".app.yaml"],
    editorComponent: "AppEditor",
  },
  [FileType.SQL]: {
    type: FileType.SQL,
    extensions: [".sql"],
    editorComponent: "SqlEditor",
  },
  [FileType.DEFAULT]: {
    type: FileType.DEFAULT,
    extensions: [],
    editorComponent: "DefaultEditor",
  },
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
    return atob(pathb64);
  } catch {
    return "";
  }
};
