import { Code } from "lucide-react";
import { DataAppReference } from "@/types/chat";
import { ReferenceItemContainer } from "./ReferenceItemContainer";
import { useState } from "react";
import {
  Dialog,
  DialogContent,
  DialogTrigger,
} from "@/components/ui/shadcn/dialog";
import EditorTab from "@/pages/ide/Editor/EditorTab";

export type Props = {
  reference: DataAppReference;
};

export const DataAppReferenceItem = ({ reference }: Props) => {
  const metadata = reference;
  const pathb64 = btoa(metadata.file_path);
  const [isOpen, setIsOpen] = useState(true);
  return (
    <Dialog open={isOpen} onOpenChange={setIsOpen}>
      <DialogTrigger>
        <ReferenceItemContainer isOpen={false}>
          <div className="px-4 py-2 gap-2 w-50 flex flex-col items-center justify-center overflow-hidden text-muted-foreground">
            <div className="flex text-sm items-center gap-2 justify-start w-full">
              <Code size={16} />
              <span className="truncate">Data App</span>
            </div>
            <span className="w-full text-start line-clamp-2 font-mono leading-[20px] text-sm">
              {metadata.file_path}
            </span>
          </div>
        </ReferenceItemContainer>
      </DialogTrigger>
      <DialogContent
        showOverlay={false}
        className="[&>button]:hidden break-all p-0 w-[70vw]! h-[70vh]! max-w-[70vw]! max-h-[70vh]! overflow-auto"
      >
        <EditorTab pathb64={pathb64} />
      </DialogContent>
    </Dialog>
  );
};
