import { css } from "styled-system/css";

import Button from "@/components/ui/Button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/Dropdown";
import Icon from "@/components/ui/Icon";

type Props = {
  className: string;

  onOpenChange: (open: boolean) => void;
  onDelete: () => void;
  onRename: () => void;
  onDuplicate: () => void;
  isOpen: boolean;
};

export default function MoreDropdown({
  className,
  onOpenChange,
  onDelete,
  onRename,
  onDuplicate,
  isOpen,
}: Props) {
  return (
    <DropdownMenu open={isOpen} onOpenChange={onOpenChange}>
      <DropdownMenuTrigger asChild>
        <Button
          className={className}
          content="icon"
          variant="ghost"
          data-functional
        >
          <Icon asset="more" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent
        data-functional
        align="end"
        side="bottom"
        className={css({ w: "180px" })}
      >
        <DropdownMenuItem
          onSelect={onRename}
          iconAsset="rename"
          text="Rename"
        />
        <DropdownMenuItem
          onSelect={onDuplicate}
          iconAsset="copy"
          text="Duplicate"
        />
        <DropdownMenuItem
          onSelect={onDelete}
          iconAsset="trash"
          text="Delete"
          buttonClassName={css({ color: "brand.error.colorTextDanger" })}
        />
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
