import { FileType } from "@/utils/fileTypes";

// Reuse a single collator for faster, locale-aware, case-insensitive comparisons
export const NAME_COLLATOR = new Intl.Collator(undefined, {
  sensitivity: "base",
  numeric: true,
});

export const IGNORE_FILES_REGEX = [/^docker-entrypoints/, /^output/, /^\./];

// Object file types that should appear in Objects view
export const OBJECT_FILE_TYPES = [
  FileType.WORKFLOW,
  FileType.AUTOMATION,
  FileType.AGENT,
  FileType.AGENTIC_WORKFLOW,
  FileType.APP,
  FileType.VIEW,
  FileType.TOPIC,
];

// Sub-view mode for Files section
export enum FilesSubViewMode {
  OBJECTS = "objects",
  FILES = "files",
}
