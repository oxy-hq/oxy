export interface FileTreeModel {
  name: string;
  path: string;
  is_dir: boolean;
  children: FileTreeModel[];
}
