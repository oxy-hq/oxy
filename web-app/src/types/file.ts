export interface FileTreeModel {
  name: string;
  path: string;
  is_dir: boolean;
  children: FileTreeModel[];
}

export interface FileStatus {
  path: string;
  status: "M" | "A" | "D";
  insert: number;
  delete: number;
}
