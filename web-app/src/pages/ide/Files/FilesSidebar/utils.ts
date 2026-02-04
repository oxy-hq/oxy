import {
  AppWindow,
  BookOpen,
  Bot,
  Braces,
  Eye,
  FileCode,
  Network,
  Table,
  Workflow
} from "lucide-react";
import type { FileTreeModel } from "@/types/file";
import { detectFileType, FileType } from "@/utils/fileTypes";
import { NAME_COLLATOR, OBJECT_FILE_TYPES } from "./constants";

// Helper to check if a file is an object type
export const isObjectFile = (file: FileTreeModel): boolean => {
  if (file.is_dir) return false;
  const fileType = detectFileType(file.path);
  return OBJECT_FILE_TYPES.includes(fileType);
};

// Helper to get clean object name (without extension)
export const getObjectName = (file: FileTreeModel): string => {
  const fileName = file.name;
  return fileName
    .replace(/\.(workflow|automation|agent|aw|app|view|topic)\.(yml|yaml)$/, "")
    .replace(/\.(yml|yaml)$/, "");
};

// Helper to get icon for file type
export const getFileTypeIcon = (fileType: FileType, fileName?: string) => {
  switch (fileType) {
    case FileType.WORKFLOW:
    case FileType.AUTOMATION:
      return Workflow;
    case FileType.AGENT:
      return Bot;
    case FileType.AGENTIC_WORKFLOW:
      return Network;
    case FileType.APP:
      return AppWindow;
    case FileType.VIEW:
      return Eye;
    case FileType.TOPIC:
      return BookOpen;
    case FileType.SQL:
      return FileCode;
    default:
      if (fileName?.toLowerCase().endsWith(".json")) {
        return Braces;
      }
      if (fileName?.toLowerCase().endsWith(".csv")) {
        return Table;
      }
      return null;
  }
};

interface GroupedObjects {
  automations: FileTreeModel[];
  agents: FileTreeModel[];
  apps: FileTreeModel[];
  semanticObjects: FileTreeModel[];
}

// Group objects by type
export const groupObjectsByType = (files: FileTreeModel[]): GroupedObjects => {
  const groups: GroupedObjects = {
    automations: [],
    agents: [],
    apps: [],
    semanticObjects: []
  };

  files.forEach((file) => {
    if (file.is_dir) return;
    const fileType = detectFileType(file.path);

    switch (fileType) {
      case FileType.WORKFLOW:
      case FileType.AUTOMATION:
      case FileType.AGENTIC_WORKFLOW:
        groups.automations.push(file);
        break;
      case FileType.AGENT:
        groups.agents.push(file);
        break;
      case FileType.APP:
        groups.apps.push(file);
        break;
      case FileType.VIEW:
      case FileType.TOPIC:
        groups.semanticObjects.push(file);
        break;
    }
  });

  // Sort each group alphabetically by name
  groups.automations.sort((a, b) => NAME_COLLATOR.compare(a.name, b.name));
  groups.agents.sort((a, b) => NAME_COLLATOR.compare(a.name, b.name));
  groups.apps.sort((a, b) => NAME_COLLATOR.compare(a.name, b.name));
  groups.semanticObjects.sort((a, b) => NAME_COLLATOR.compare(a.name, b.name));

  return groups;
};

// Helper to get all object files from the full file list
export const getAllObjectFiles = (allFiles: FileTreeModel[]): FileTreeModel[] => {
  const objectFiles: FileTreeModel[] = [];

  const traverse = (files: FileTreeModel[]) => {
    files.forEach((file) => {
      if (isObjectFile(file)) {
        objectFiles.push(file);
      }
      if (file.is_dir && file.children) {
        traverse(file.children);
      }
    });
  };

  traverse(allFiles);
  return objectFiles;
};
