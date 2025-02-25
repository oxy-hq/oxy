import { css } from "styled-system/css";

import Button from "@/components/ui/Button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/Dropdown";
import Icon from "@/components/ui/Icon";
import { useState } from "react";
import { AgentContext } from "../type";
import ContextForm from "./ContextForm";

type Props = {
  onAddContext: (value: AgentContext) => void;
};

export default function AddContextMenu({ onAddContext }: Props) {
  const [open, setOpen] = useState(false);
  const [fileModalOpen, setFileModalOpen] = useState(false);
  const [semanticModelModalOpen, setSemanticModelModalOpen] = useState(false);

  return (
    <div>
      <DropdownMenu open={open} onOpenChange={setOpen}>
        <DropdownMenuTrigger asChild>
          <Button content="icon" variant="ghost">
            <Icon asset="add" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent
          data-functional
          align="end"
          side="bottom"
          className={css({ w: "180px" })}
        >
          <DropdownMenuItem
            onSelect={() => {
              setFileModalOpen(true);
            }}
            iconAsset="file"
            text="File"
          />

          <DropdownMenuItem
            onSelect={() => {
              setSemanticModelModalOpen(true);
            }}
            iconAsset="file"
            text="Semantic Model"
          />
        </DropdownMenuContent>
      </DropdownMenu>

      <ContextForm
        type="file"
        onUpdate={(data) => {
          onAddContext(data);
        }}
        open={fileModalOpen}
        onOpenChange={setFileModalOpen}
      />

      <ContextForm
        type="semantic_model"
        onUpdate={(data) => {
          onAddContext(data);
        }}
        open={semanticModelModalOpen}
        onOpenChange={setSemanticModelModalOpen}
      />
    </div>
  );
}
