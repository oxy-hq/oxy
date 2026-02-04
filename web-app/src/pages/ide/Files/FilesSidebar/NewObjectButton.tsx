import { AppWindow, BookOpen, Bot, Eye, Layers2, Plus, Workflow } from "lucide-react";
import type React from "react";
import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle
} from "@/components/ui/shadcn/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger
} from "@/components/ui/shadcn/dropdown-menu";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import useCreateFile from "@/hooks/api/files/useCreateFile";
import useFileTree from "@/hooks/api/files/useFileTree";
import useSaveFile from "@/hooks/api/files/useSaveFile";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import ROUTES from "@/libs/utils/routes";
import type { FileTreeModel } from "@/types/file";

type ObjectType = "agent" | "workflow" | "view" | "topic" | "app";

interface ObjectTypeConfig {
  type: ObjectType;
  label: string;
  icon: React.ElementType;
  extension: string;
  defaultContent: string;
}

const OBJECT_TYPE_CONFIGS: ObjectTypeConfig[] = [
  {
    type: "agent",
    label: "Agent",
    icon: Bot,
    extension: ".agent.yml",
    defaultContent: `model: ""
description: ""

system_instructions: |
  You are a helpful assistant.

tools: []
`
  },
  {
    type: "workflow",
    label: "Automation",
    icon: Workflow,
    extension: ".workflow.yml",
    defaultContent: `name: ""
description: ""

tasks: []
`
  },
  {
    type: "app",
    label: "App",
    icon: AppWindow,
    extension: ".app.yml",
    defaultContent: `tasks:

display:
`
  }
];

const SEMANTIC_LAYER_CONFIGS: ObjectTypeConfig[] = [
  {
    type: "view",
    label: "View",
    icon: Eye,
    extension: ".view.yml",
    defaultContent: `name: ""
description: ""
datasource: ""
table: ""

dimensions: []

measures: []

entities: []
`
  },
  {
    type: "topic",
    label: "Topic",
    icon: BookOpen,
    extension: ".topic.yml",
    defaultContent: `name: ""
description: ""

views: []
`
  }
];

interface NewObjectButtonProps {
  disabled?: boolean;
}

const NewObjectButton: React.FC<NewObjectButtonProps> = ({ disabled }) => {
  const [selectedType, setSelectedType] = useState<ObjectTypeConfig | null>(null);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [fileName, setFileName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [isCreating, setIsCreating] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const { data: fileTree, refetch } = useFileTree();

  useEffect(() => {
    if (dialogOpen) {
      // Small delay to ensure dialog is fully rendered
      const timer = setTimeout(() => {
        inputRef.current?.focus();
      }, 0);
      return () => clearTimeout(timer);
    }
  }, [dialogOpen]);
  const { project } = useCurrentProjectBranch();
  const createFile = useCreateFile();
  const saveFile = useSaveFile();
  const navigate = useNavigate();

  const handleTypeSelect = (config: ObjectTypeConfig) => {
    setSelectedType(config);
    setFileName("");
    setError(null);
    setDialogOpen(true);
  };

  const validateFileName = (name: string): boolean => {
    if (!name.trim()) {
      setError("File name is required");
      return false;
    }

    // Check for invalid characters
    if (/[<>:"/\\|?*]/.test(name)) {
      setError("File name contains invalid characters");
      return false;
    }

    if (!selectedType) return false;

    const fullPath = `${name}${selectedType.extension}`;

    // Check if file already exists in tree
    const pathExistsInTree = (items: FileTreeModel[]): boolean => {
      for (const item of items) {
        if (item.path === fullPath || item.name === fullPath) return true;
        if (item.children?.length) {
          if (pathExistsInTree(item.children)) return true;
        }
      }
      return false;
    };

    if (pathExistsInTree(fileTree || [])) {
      setError("A file with this name already exists");
      return false;
    }

    setError(null);
    return true;
  };

  const handleCreate = async () => {
    if (!selectedType || !validateFileName(fileName)) return;

    setIsCreating(true);
    try {
      const fullPath = `${fileName}${selectedType.extension}`;
      const pathb64 = btoa(fullPath);

      // Create the empty file first
      await createFile.mutateAsync(pathb64);

      // Then save the default content
      await saveFile.mutateAsync({
        pathb64,
        data: selectedType.defaultContent
      });

      // Refresh file tree
      await refetch();

      // Close dialog
      setDialogOpen(false);
      setSelectedType(null);
      setFileName("");

      // Navigate to the file
      navigate(ROUTES.PROJECT(project.id).IDE.FILES.FILE(pathb64));
    } catch (err) {
      toast.error("Failed to create file", {
        description: err instanceof Error ? err.message : "There was a problem creating the file."
      });
      console.error("Failed to create file:", err);
    } finally {
      setIsCreating(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      e.preventDefault();
      handleCreate();
    }
  };

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant='ghost'
            size='sm'
            disabled={disabled}
            tooltip={disabled ? "Read-only mode" : "New Object"}
          >
            <Plus className='h-4 w-4' />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align='start'>
          {OBJECT_TYPE_CONFIGS.map((config) => (
            <DropdownMenuItem key={config.type} onClick={() => handleTypeSelect(config)}>
              <config.icon className='mr-2 h-4 w-4' />
              {config.label}
            </DropdownMenuItem>
          ))}
          <DropdownMenuSub>
            <DropdownMenuSubTrigger className='px-3 py-2'>
              <Layers2 className='mr-4 h-4 w-4' />
              Semantic Layer
              <div className='mr-2'></div>
            </DropdownMenuSubTrigger>
            <DropdownMenuSubContent>
              {SEMANTIC_LAYER_CONFIGS.map((config) => (
                <DropdownMenuItem key={config.type} onClick={() => handleTypeSelect(config)}>
                  <config.icon className='mr-2 h-4 w-4' />
                  {config.label}
                </DropdownMenuItem>
              ))}
            </DropdownMenuSubContent>
          </DropdownMenuSub>
        </DropdownMenuContent>
      </DropdownMenu>

      <Dialog open={dialogOpen} onOpenChange={setDialogOpen}>
        <DialogContent className='sm:max-w-md'>
          <DialogHeader>
            <DialogTitle className='flex items-center gap-2'>
              {selectedType && <selectedType.icon className='h-5 w-5' />}
              New {selectedType?.label}
            </DialogTitle>
          </DialogHeader>
          <div className='grid gap-4 py-4'>
            <div className='grid gap-2'>
              <Label htmlFor='fileName'>Name</Label>
              <div className='flex items-center gap-2'>
                <Input
                  id='fileName'
                  ref={inputRef}
                  value={fileName}
                  onChange={(e) => {
                    setFileName(e.target.value);
                    setError(null);
                  }}
                  onKeyDown={handleKeyDown}
                  placeholder='my-file'
                  className={error ? "border-destructive" : ""}
                />
                <span className='whitespace-nowrap text-muted-foreground text-sm'>
                  {selectedType?.extension}
                </span>
              </div>
              {error && <p className='text-destructive text-sm'>{error}</p>}
            </div>
          </div>
          <DialogFooter>
            <Button variant='outline' onClick={() => setDialogOpen(false)} disabled={isCreating}>
              Cancel
            </Button>
            <Button onClick={handleCreate} disabled={isCreating || !fileName.trim()}>
              {isCreating ? "Creating..." : "Create"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
};

export default NewObjectButton;
