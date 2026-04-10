import { useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList
} from "@/components/ui/shadcn/command";
import useFileTree from "@/hooks/api/files/useFileTree";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { encodeBase64 } from "@/libs/encoding";
import ROUTES from "@/libs/utils/routes";
import { IGNORE_FILES_REGEX } from "@/pages/ide/Files/FilesSidebar/constants";
import { getFileTypeIcon } from "@/pages/ide/Files/FilesSidebar/utils";
import useFileQuickOpen from "@/stores/useFileQuickOpen";
import type { FileTreeModel } from "@/types/file";
import { detectFileType } from "@/utils/fileTypes";

function flattenFiles(files: FileTreeModel[]): FileTreeModel[] {
  const result: FileTreeModel[] = [];
  const traverse = (nodes: FileTreeModel[]) => {
    for (const node of nodes) {
      if (IGNORE_FILES_REGEX.some((r) => node.name.match(r))) continue;
      if (!node.is_dir) result.push(node);
      if (node.is_dir && node.children) traverse(node.children);
    }
  };
  traverse(files);
  return result;
}

export function FileQuickOpen() {
  const { isOpen, setIsOpen } = useFileQuickOpen();
  const { project } = useCurrentProjectBranch();
  const navigate = useNavigate();
  const [query, setQuery] = useState("");

  const { data: fileTreeData } = useFileTree(isOpen);

  const allFiles = useMemo(() => {
    if (!fileTreeData) return [];
    return flattenFiles(fileTreeData.primary);
  }, [fileTreeData]);

  const filtered = useMemo(() => {
    if (!query) return allFiles.slice(0, 50);
    const q = query.toLowerCase();
    return allFiles
      .filter((f) => f.path.toLowerCase().includes(q) || f.name.toLowerCase().includes(q))
      .slice(0, 50);
  }, [query, allFiles]);

  const handleSelect = (file: FileTreeModel) => {
    setIsOpen(false);
    setQuery("");
    navigate(ROUTES.WORKSPACE(project.id).IDE.FILES.FILE(encodeBase64(file.path)));
  };

  const handleOpenChange = (open: boolean) => {
    setIsOpen(open);
    if (!open) setQuery("");
  };

  return (
    <CommandDialog
      open={isOpen}
      onOpenChange={handleOpenChange}
      title='Go to file'
      description='Search for a file to open'
      className='border-white/10 bg-background/50 shadow-[0_8px_60px_rgba(0,0,0,0.5),0_0_0_1px_rgba(255,255,255,0.08),0_0_30px_-4px_color-mix(in_srgb,var(--primary)_15%,transparent)] backdrop-blur-2xl'
    >
      <CommandInput placeholder='Go to file…' value={query} onValueChange={setQuery} />
      <CommandList>
        <CommandEmpty>No files found.</CommandEmpty>
        <CommandGroup>
          {filtered.map((file) => {
            const fileType = detectFileType(file.path);
            const Icon = getFileTypeIcon(fileType, file.name);
            return (
              <CommandItem
                key={file.path}
                value={file.path}
                onSelect={() => handleSelect(file)}
                className='flex items-center gap-2'
              >
                {Icon && <Icon className='size-4 shrink-0 text-muted-foreground' />}
                <span className='truncate'>{file.name}</span>
                <span className='ml-auto truncate text-muted-foreground text-xs'>{file.path}</span>
              </CommandItem>
            );
          })}
        </CommandGroup>
      </CommandList>
    </CommandDialog>
  );
}
