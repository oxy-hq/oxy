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
import { ToolConfig } from "../type";
import ToolForm from "./ToolForm";

type Props = {
  onAddTool: (value: ToolConfig) => void;
};

export default function AddToolMenu({ onAddTool }: Props) {
  const [open, setOpen] = useState(false);
  const [executeSqlOpen, setExecuteSqlOpen] = useState(false);
  const [validateSqlOpen, setValidateSqlOpen] = useState(false);
  const [retrievalOpen, setRetrievalOpen] = useState(false);

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
              setExecuteSqlOpen(true);
            }}
            iconAsset="file"
            text="Execute SQL"
          />

          <DropdownMenuItem
            onSelect={() => {
              setValidateSqlOpen(true);
            }}
            iconAsset="check"
            text="Validate SQL"
          />

          <DropdownMenuItem
            onSelect={() => {
              setRetrievalOpen(true);
            }}
            iconAsset="search"
            text="Retrieval"
          />
        </DropdownMenuContent>
      </DropdownMenu>

      <ToolForm
        open={executeSqlOpen}
        onOpenChange={setExecuteSqlOpen}
        type="execute_sql"
        onUpdate={(data) => {
          onAddTool(data);
        }}
      />

      <ToolForm
        open={validateSqlOpen}
        onOpenChange={setValidateSqlOpen}
        type="validate_sql"
        onUpdate={(data) => {
          onAddTool(data);
        }}
      />

      <ToolForm
        open={retrievalOpen}
        onOpenChange={setRetrievalOpen}
        type="retrieval"
        onUpdate={(data) => {
          onAddTool(data);
        }}
      />
    </div>
  );
}
