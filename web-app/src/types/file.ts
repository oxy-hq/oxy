export interface FileTreeModel {
  name: string;
  path: string;
  is_dir: boolean;
  children: FileTreeModel[];
}

export interface FileStatus {
  path: string;
  status: "M" | "A" | "D" | "U";
  insert: number;
  delete: number;
}
